use super::*;
use crate::stream::{collect_data, ByteStream};
use crate::vault_key_change::types::Entry;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct VersionedStore {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    history: Mutex<Vec<(String, Vec<u8>)>>,
    writes: Mutex<usize>,
    lose_write_ack: Mutex<bool>,
    incomplete_list: Mutex<bool>,
    after_write: Option<Arc<tokio::sync::Barrier>>,
}

#[async_trait]
impl RekeyStorage for VersionedStore {
    async fn list(&self) -> Result<Vec<Entry>> {
        if *self.incomplete_list.lock().unwrap() {
            return Ok(vec![]);
        }
        Ok(self
            .objects
            .lock()
            .unwrap()
            .iter()
            .map(|(key, value)| Entry {
                key: key.clone(),
                size: value.len() as u64,
                etag: "etag".into(),
            })
            .collect())
    }
    async fn read(&self, key: &str) -> Result<ByteStream> {
        Ok(byte_stream(
            self.objects
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or_else(|| error("missing"))?,
        ))
    }
    async fn head(&self, key: &str) -> Result<Option<Entry>> {
        Ok(self.objects.lock().unwrap().get(key).map(|value| Entry {
            key: key.into(),
            size: value.len() as u64,
            etag: "etag".into(),
        }))
    }
    async fn write(&self, key: &str, _size: u64, stream: ByteStream) -> Result<String> {
        let bytes = collect_data(stream).await.map_err(error)?;
        if let Some(old) = self
            .objects
            .lock()
            .unwrap()
            .insert(key.into(), bytes.clone())
        {
            self.history.lock().unwrap().push((key.into(), old));
        }
        *self.writes.lock().unwrap() += 1;
        if std::mem::take(&mut *self.lose_write_ack.lock().unwrap()) {
            return Err(error("写入成功但响应丢失"));
        }
        if let Some(barrier) = &self.after_write {
            barrier.wait().await;
        }
        Ok("etag".into())
    }
    async fn delete(&self, key: &str) -> Result<()> {
        // 模拟版本控制桶的删除：当前对象消失，但旧字节仍在历史中。
        if let Some(old) = self.objects.lock().unwrap().remove(key) {
            self.history.lock().unwrap().push((key.into(), old));
        }
        Ok(())
    }
    async fn checkpoint(&self, _journal: &str) -> Result<()> {
        Ok(())
    }
}

fn claim(id: char) -> CloudClaim {
    CloudClaim::new(&id.to_string().repeat(32), &"f".repeat(32)).unwrap()
}

#[tokio::test]
async fn acquires_resumes_and_releases_without_erasing_history() {
    let store = VersionedStore::default();
    let owner = claim('a');
    store
        .write("diaries/1/manifest.enc", 3, byte_stream(b"old".to_vec()))
        .await
        .unwrap();
    store
        .write("diaries/1/manifest.enc", 3, byte_stream(b"new".to_vec()))
        .await
        .unwrap();
    assert!(owner.acquire(&store).await.unwrap());
    let writes = *store.writes.lock().unwrap();
    assert!(owner.acquire(&store).await.unwrap());
    assert_eq!(*store.writes.lock().unwrap(), writes);
    owner.release(&store).await.unwrap();
    owner.release(&store).await.unwrap();
    assert_eq!(store.history.lock().unwrap().len(), 2);
    assert_eq!(
        collect_data(store.read("diaries/1/manifest.enc").await.unwrap())
            .await
            .unwrap(),
        b"new"
    );
    assert!(store.head(&owner.key()).await.unwrap().is_none());
    assert!(claim('b').acquire(&store).await.unwrap());
}

#[tokio::test]
async fn contender_cannot_replace_an_owner_even_when_its_id_is_smaller() {
    let store = VersionedStore::default();
    let owner = claim('b');
    let other = claim('a');
    assert!(owner.acquire(&store).await.unwrap());
    assert!(!other.acquire(&store).await.unwrap());
    other.release(&store).await.unwrap();
    assert!(owner.acquire(&store).await.unwrap());
    assert!(store.head(&owner.key()).await.unwrap().is_some());
}

#[tokio::test]
async fn resumes_a_claim_whose_write_ack_was_lost_and_rejects_a_missing_list_result() {
    let store = VersionedStore::default();
    let owner = claim('a');
    *store.lose_write_ack.lock().unwrap() = true;
    assert!(owner.acquire(&store).await.is_err());
    assert!(owner.acquire(&store).await.unwrap());
    assert_eq!(*store.writes.lock().unwrap(), 1);
    *store.incomplete_list.lock().unwrap() = true;
    assert!(owner.acquire(&store).await.is_err());
}

#[tokio::test]
async fn corrupt_or_foreign_claim_is_never_overwritten_or_deleted() {
    for bytes in [
        b"invalid json".to_vec(),
        serde_json::to_vec(&claim('b')).unwrap(),
    ] {
        let store = VersionedStore::default();
        let owner = claim('a');
        store
            .objects
            .lock()
            .unwrap()
            .insert(owner.key(), bytes.clone());
        assert!(owner.acquire(&store).await.is_err());
        assert!(owner.release(&store).await.is_err());
        assert_eq!(
            store.objects.lock().unwrap().get(&owner.key()),
            Some(&bytes)
        );
        assert_eq!(*store.writes.lock().unwrap(), 0);
    }
    for id in ["", "../unsafe", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"] {
        assert!(CloudClaim::new(id, &"f".repeat(32)).is_err());
    }
}

#[test]
fn all_two_contender_interleavings_allow_at_most_one_active_owner() {
    // 每方按“写自己占位 -> 枚举决定 -> 失败则撤回自己”前进。
    // 穷举所有合法交错；获准的一方在业务完成之前绝不释放占位。
    fn explore(steps: [u8; 2], present: [bool; 2], admitted: [bool; 2], paths: &mut usize) {
        assert!(admitted.iter().filter(|&&active| active).count() <= 1);
        if steps == [3, 3] {
            *paths += 1;
            return;
        }
        for client in 0..2 {
            if steps[client] == 3 {
                continue;
            }
            let mut next_steps = steps;
            let mut next_present = present;
            let mut next_admitted = admitted;
            match steps[client] {
                0 => next_present[client] = true,
                1 => next_admitted[client] = present[client] && !present[1 - client],
                2 if !admitted[client] => next_present[client] = false,
                _ => (),
            }
            next_steps[client] += 1;
            explore(next_steps, next_present, next_admitted, paths);
        }
    }
    let mut paths = 0;
    explore([0, 0], [false, false], [false, false], &mut paths);
    assert_eq!(paths, 20);
}

#[tokio::test]
async fn concurrent_publications_stop_both_contenders_without_erasing_each_other() {
    let store = VersionedStore {
        after_write: Some(Arc::new(tokio::sync::Barrier::new(2))),
        ..Default::default()
    };
    let a = claim('a');
    let b = claim('b');
    let (left, right) = tokio::join!(a.acquire(&store), b.acquire(&store));
    assert!(!left.unwrap());
    assert!(!right.unwrap());
    a.release(&store).await.unwrap();
    assert!(store.head(&b.key()).await.unwrap().is_some());
    b.release(&store).await.unwrap();
    assert!(store.list().await.unwrap().is_empty());
    assert_eq!(store.history.lock().unwrap().len(), 2);
}

#[test]
fn all_three_contender_interleavings_remain_exclusive_through_completion() {
    fn explore(steps: [u8; 3], present: [bool; 3], active: [bool; 3], paths: &mut usize) {
        assert!(active.iter().filter(|&&value| value).count() <= 1);
        if steps == [4, 4, 4] {
            *paths += 1;
            return;
        }
        for client in 0..3 {
            if steps[client] == 4 {
                continue;
            }
            let mut next_steps = steps;
            let mut next_present = present;
            let mut next_active = active;
            match steps[client] {
                0 => next_present[client] = true,
                1 => {
                    next_active[client] =
                        present[client] && present.iter().filter(|&&value| value).count() == 1
                }
                2 if !active[client] => next_present[client] = false,
                3 => {
                    next_present[client] = false;
                    next_active[client] = false;
                }
                _ => (),
            }
            next_steps[client] += 1;
            explore(next_steps, next_present, next_active, paths);
        }
    }
    let mut paths = 0;
    explore([0; 3], [false; 3], [false; 3], &mut paths);
    assert_eq!(paths, 34_650);
}

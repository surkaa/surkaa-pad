use crate::caches::{CacheError, LocalObjectStore};
use crate::object::{ObjectError, OssClient};
use crate::object_locations::{ObjectLocations, StoredObject, StoredObjectCollection};
use async_trait::async_trait;
use std::sync::Arc;
use tauri_plugin_log::log;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppObjectStoreError {
    #[error("本地对象存储操作失败: {0}")]
    Local(#[from] CacheError),
    #[error("远程对象存储操作失败: {0}")]
    Remote(#[from] ObjectError),
}

/// 面向应用领域对象的字节存储。
///
/// 上层只传递 `StoredObject`，对象 Key 的布局规则统一留在 `ObjectLocations`；JSON、
/// 加密及领域校验由各自的 Repository 负责。
#[async_trait]
pub trait AppObjectStore: Send + Sync {
    async fn load_bytes(
        &self,
        object: &StoredObject,
    ) -> Result<Option<Vec<u8>>, AppObjectStoreError>;

    async fn save_bytes(
        &self,
        object: &StoredObject,
        data: &[u8],
    ) -> Result<(), AppObjectStoreError>;

    async fn delete(&self, object: &StoredObject) -> Result<(), AppObjectStoreError>;

    async fn list(
        &self,
        collection: &StoredObjectCollection,
    ) -> Result<Vec<StoredObject>, AppObjectStoreError>;
}

#[derive(Clone)]
pub struct LocalAppObjectStore {
    local: LocalObjectStore,
}

impl LocalAppObjectStore {
    pub fn new(local: LocalObjectStore) -> Self {
        Self { local }
    }
}

#[async_trait]
impl AppObjectStore for LocalAppObjectStore {
    async fn load_bytes(
        &self,
        object: &StoredObject,
    ) -> Result<Option<Vec<u8>>, AppObjectStoreError> {
        let key = ObjectLocations::key(object);
        if self.local.get(&key).await?.is_none() {
            return Ok(None);
        }
        match self.local.get_data(&key).await {
            Ok(data) => Ok(Some(data)),
            Err(CacheError::NotFound) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn save_bytes(
        &self,
        object: &StoredObject,
        data: &[u8],
    ) -> Result<(), AppObjectStoreError> {
        self.local
            .save_bytes(&ObjectLocations::key(object), data)
            .await?;
        Ok(())
    }

    async fn delete(&self, object: &StoredObject) -> Result<(), AppObjectStoreError> {
        self.local.delete(&ObjectLocations::key(object)).await?;
        Ok(())
    }

    async fn list(
        &self,
        collection: &StoredObjectCollection,
    ) -> Result<Vec<StoredObject>, AppObjectStoreError> {
        let prefix = collection_prefix(collection);
        let mut objects = self
            .local
            .get_entries_with_prefix(&prefix)
            .await?
            .into_iter()
            .filter_map(|entry| ObjectLocations::parse(&entry.key))
            .filter(|object| collection.contains(object))
            .collect::<Vec<_>>();
        sort_and_deduplicate(&mut objects);
        Ok(objects)
    }
}

/// 远程模式以 OSS 为权威存储，LOS 只作为可丢弃缓存。
#[derive(Clone)]
pub struct RemoteAppObjectStore {
    remote: OssClient,
    local: LocalObjectStore,
}

impl RemoteAppObjectStore {
    pub fn new(remote: OssClient, local: LocalObjectStore) -> Self {
        Self { remote, local }
    }

    async fn cache_download(&self, key: &str, data: &[u8], etag: Option<&str>) {
        if let Err(error) = self.local.save_bytes(key, data).await {
            log::warn!("[app object store] cache write failed: key={key}, error={error}");
            return;
        }
        if let Some(etag) = etag.filter(|etag| !etag.trim().is_empty()) {
            if let Err(error) = self.local.set_etag(key, etag).await {
                log::warn!("[app object store] cache etag write failed: key={key}, error={error}");
            }
        }
    }
}

#[async_trait]
impl AppObjectStore for RemoteAppObjectStore {
    async fn load_bytes(
        &self,
        object: &StoredObject,
    ) -> Result<Option<Vec<u8>>, AppObjectStoreError> {
        let key = ObjectLocations::key(object);
        if !self.remote.object_exists(&key).await? {
            if let Err(error) = self.local.delete(&key).await {
                log::warn!(
                    "[app object store] stale cache cleanup failed: key={key}, error={error}"
                );
            }
            return Ok(None);
        }

        let metadata = self.remote.get_metadata(&key).await?;
        let local_etag = match self.local.get(&key).await {
            Ok(etag) => etag,
            Err(error) => {
                log::warn!(
                    "[app object store] cache metadata read failed, downloading again: key={key}, error={error}"
                );
                None
            }
        };
        if let (Some(remote_etag), Some(local_etag)) = (metadata.etag.as_deref(), local_etag) {
            if etags_match(remote_etag, &local_etag) {
                match self.local.get_data(&key).await {
                    Ok(data) => return Ok(Some(data)),
                    Err(error) => log::warn!(
                        "[app object store] valid cache read failed, downloading again: key={key}, error={error}"
                    ),
                }
            }
        }

        let data = self.remote.download_bytes(&key).await?;
        self.cache_download(&key, &data, metadata.etag.as_deref())
            .await;
        Ok(Some(data))
    }

    async fn save_bytes(
        &self,
        object: &StoredObject,
        data: &[u8],
    ) -> Result<(), AppObjectStoreError> {
        let key = ObjectLocations::key(object);
        let etag = self.remote.upload_bytes(&key, data).await?;
        self.cache_download(&key, data, Some(&etag)).await;
        Ok(())
    }

    async fn delete(&self, object: &StoredObject) -> Result<(), AppObjectStoreError> {
        let key = ObjectLocations::key(object);
        self.remote.delete(&key).await?;
        if let Err(error) = self.local.delete(&key).await {
            log::warn!("[app object store] cache delete failed: key={key}, error={error}");
        }
        Ok(())
    }

    async fn list(
        &self,
        collection: &StoredObjectCollection,
    ) -> Result<Vec<StoredObject>, AppObjectStoreError> {
        let mut objects = match collection {
            StoredObjectCollection::AiSessionMetas => {
                let mut sessions = Vec::new();
                let mut token = None;
                loop {
                    let (prefixes, next_token) = self
                        .remote
                        .list_common_prefixes(ObjectLocations::ai_sessions_prefix(), token)
                        .await?;
                    for prefix in prefixes {
                        let Some(session_id) =
                            ObjectLocations::ai_session_id_from_common_prefix(&prefix)
                        else {
                            continue;
                        };
                        let object = StoredObject::AiSessionMeta { session_id };
                        if self
                            .remote
                            .object_exists(&ObjectLocations::key(&object))
                            .await?
                        {
                            sessions.push(object);
                        }
                    }
                    token = next_token;
                    if token.is_none() {
                        break;
                    }
                }
                sessions
            }
            StoredObjectCollection::AiSessionMessageBlocks { session_id } => {
                let mut blocks = Vec::new();
                let mut token = None;
                let prefix = ObjectLocations::ai_session_messages_prefix(session_id);
                loop {
                    let (page, next_token) = self.remote.list(&prefix, token).await?;
                    blocks.extend(
                        page.into_iter()
                            .filter_map(|entry| ObjectLocations::parse(&entry.key))
                            .filter(|object| collection.contains(object)),
                    );
                    token = next_token;
                    if token.is_none() {
                        break;
                    }
                }
                blocks
            }
        };
        sort_and_deduplicate(&mut objects);
        Ok(objects)
    }
}

fn collection_prefix(collection: &StoredObjectCollection) -> String {
    match collection {
        StoredObjectCollection::AiSessionMetas => ObjectLocations::ai_sessions_prefix().to_owned(),
        StoredObjectCollection::AiSessionMessageBlocks { session_id } => {
            ObjectLocations::ai_session_messages_prefix(session_id)
        }
    }
}

fn sort_and_deduplicate(objects: &mut Vec<StoredObject>) {
    objects.sort_by_key(ObjectLocations::key);
    objects.dedup();
}

fn etags_match(left: &str, right: &str) -> bool {
    left.trim_matches('"')
        .eq_ignore_ascii_case(right.trim_matches('"'))
}

pub type SharedAppObjectStore = Arc<dyn AppObjectStore>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestOssGuard;

    #[tokio::test]
    async fn local_store_uses_canonical_paths_and_filters_domain_collections() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = LocalAppObjectStore::new(local.clone());
        let meta = StoredObject::AiSessionMeta {
            session_id: "1".into(),
        };
        let matching_block = StoredObject::AiSessionMessageBlock {
            session_id: "1".into(),
            level: 0,
            block_id: 2,
        };
        let other_block = StoredObject::AiSessionMessageBlock {
            session_id: "2".into(),
            level: 0,
            block_id: 0,
        };

        assert_eq!(store.load_bytes(&meta).await.unwrap(), None);
        store.save_bytes(&meta, b"meta").await.unwrap();
        store
            .save_bytes(&matching_block, b"matching")
            .await
            .unwrap();
        store.save_bytes(&other_block, b"other").await.unwrap();
        local
            .save_bytes("diaries/1/manifest.enc", b"diary")
            .await
            .unwrap();

        assert_eq!(
            local
                .get_data("ai/sessions/1/messages/0/2.enc")
                .await
                .unwrap(),
            b"matching"
        );
        assert_eq!(
            store.load_bytes(&meta).await.unwrap(),
            Some(b"meta".to_vec())
        );
        assert_eq!(
            store
                .list(&StoredObjectCollection::AiSessionMetas)
                .await
                .unwrap(),
            vec![meta.clone()]
        );
        assert_eq!(
            store
                .list(&StoredObjectCollection::AiSessionMessageBlocks {
                    session_id: "1".into(),
                })
                .await
                .unwrap(),
            vec![matching_block]
        );

        store.delete(&meta).await.unwrap();
        assert_eq!(store.load_bytes(&meta).await.unwrap(), None);
    }

    #[test]
    fn compares_quoted_etags_case_insensitively() {
        assert!(etags_match("\"ABCD\"", "abcd"));
        assert!(!etags_match("abcd", "efgh"));
    }

    #[tokio::test]
    async fn remote_store_uses_oss_as_authority_and_lists_only_complete_sessions() {
        let remote = OssClient::from_env();
        let (remote, guard) = TestOssGuard::new(remote).await;
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = RemoteAppObjectStore::new(remote.clone(), local.clone());
        let meta = StoredObject::AiSessionMeta {
            session_id: "1".into(),
        };
        let block = StoredObject::AiSessionMessageBlock {
            session_id: "1".into(),
            level: 0,
            block_id: 0,
        };
        let orphan = StoredObject::AiSessionMessageBlock {
            session_id: "2".into(),
            level: 0,
            block_id: 0,
        };

        store.save_bytes(&meta, b"meta-v1").await.unwrap();
        store.save_bytes(&block, b"block").await.unwrap();
        store.save_bytes(&orphan, b"orphan").await.unwrap();
        assert_eq!(
            store
                .list(&StoredObjectCollection::AiSessionMetas)
                .await
                .unwrap(),
            vec![meta.clone()]
        );
        assert_eq!(
            store
                .list(&StoredObjectCollection::AiSessionMessageBlocks {
                    session_id: "1".into(),
                })
                .await
                .unwrap(),
            vec![block]
        );

        // 绕过缓存更新远端，读取时必须根据 ETag 放弃旧缓存。
        remote
            .upload_bytes(&ObjectLocations::key(&meta), b"meta-v2")
            .await
            .unwrap();
        assert_eq!(
            store.load_bytes(&meta).await.unwrap(),
            Some(b"meta-v2".to_vec())
        );
        assert_eq!(
            local.get_data(&ObjectLocations::key(&meta)).await.unwrap(),
            b"meta-v2"
        );

        // 远端不存在时，即使 LOS 残留旧副本也不能让对象重新出现。
        remote.delete(&ObjectLocations::key(&meta)).await.unwrap();
        assert_eq!(store.load_bytes(&meta).await.unwrap(), None);
        assert!(local
            .get(&ObjectLocations::key(&meta))
            .await
            .unwrap()
            .is_none());

        guard.cleanup().await;
    }
}

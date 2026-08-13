use crate::app_config::AppConfigStore;
use crate::caches::{CacheError, LocalObjectStore};
use crate::storages::is_diary_attachment_key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

const CACHE_INDEX_FILENAME: &str = ".attachment-cache-index.json";
const CACHE_INDEX_VERSION: u32 = 1;
const ACCESS_PERSIST_INTERVAL_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachmentCacheStats {
    pub cached_files: u32,
    pub cached_bytes: u64,
    pub limit_bytes: u64,
    pub max_file_size_bytes: u64,
}

#[derive(Clone)]
pub struct AttachmentCacheManager {
    los: LocalObjectStore,
    app_config: AppConfigStore,
    state: Arc<Mutex<CacheState>>,
}

#[derive(Default)]
struct CacheState {
    loaded: bool,
    index: CacheIndex,
    reservations: HashMap<String, u64>,
    last_persisted_at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
    #[serde(default = "cache_index_version")]
    version: u32,
    #[serde(default)]
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntry {
    size: u64,
    #[serde(rename = "lastAccessedAtMs")]
    last_accessed_at_ms: u64,
}

fn cache_index_version() -> u32 {
    CACHE_INDEX_VERSION
}

impl AttachmentCacheManager {
    pub fn new(los: LocalObjectStore, app_config: AppConfigStore) -> Self {
        Self {
            los,
            app_config,
            state: Arc::new(Mutex::new(CacheState::default())),
        }
    }

    /// 重新扫描可从云端恢复的附件，并立即把本地缓存收敛到配置上限。
    pub async fn activate(&self) -> Result<AttachmentCacheStats, CacheError> {
        let mut state = self.state.lock().await;
        state.loaded = false;
        state.reservations.clear();
        self.ensure_loaded(&mut state).await?;
        self.evict_oversized(&mut state).await?;
        let limit_bytes = self.limit_bytes();
        self.evict_until_fits(&mut state, None, 0, limit_bytes)
            .await?;
        self.persist(&mut state).await?;
        Ok(stats(&state.index, limit_bytes, self.max_file_size_bytes()))
    }

    /// 本地模式下附件不是缓存，清除索引但不删除任何对象。
    pub async fn deactivate(&self) -> Result<(), CacheError> {
        let mut state = self.state.lock().await;
        state.loaded = true;
        state.index = CacheIndex::default();
        state.reservations.clear();
        state.last_persisted_at_ms = now_ms();
        remove_file_if_exists(&self.index_path()).await
    }

    pub async fn enforce_limit(&self) -> Result<AttachmentCacheStats, CacheError> {
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        self.evict_oversized(&mut state).await?;
        let limit_bytes = self.limit_bytes();
        self.evict_until_fits(&mut state, None, 0, limit_bytes)
            .await?;
        self.persist(&mut state).await?;
        Ok(stats(&state.index, limit_bytes, self.max_file_size_bytes()))
    }

    /// 为即将写入的附件预留容量。预留期间不会把同一对象当作可淘汰候选。
    pub async fn reserve(&self, key: &str, size: u64) -> Result<(), CacheError> {
        if !is_diary_attachment_key(key) {
            return Ok(());
        }
        self.ensure_file_cacheable(size)?;
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        let limit_bytes = self.limit_bytes();
        if size > limit_bytes {
            return Err(CacheError::CapacityExceeded {
                required_bytes: size,
                limit_bytes,
            });
        }
        if state.reservations.contains_key(key) {
            return Err(CacheError::Metadata(format!(
                "附件已存在进行中的缓存写入: {key}"
            )));
        }

        self.evict_until_fits(&mut state, Some(key), size, limit_bytes)
            .await?;
        self.persist(&mut state).await?;
        state.reservations.insert(key.to_string(), size);
        Ok(())
    }

    pub async fn commit(&self, key: &str, actual_size: u64) -> Result<(), CacheError> {
        if !is_diary_attachment_key(key) {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        state.reservations.remove(key);
        if let Err(error) = self.ensure_file_cacheable(actual_size) {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Err(error);
        }
        let limit_bytes = self.limit_bytes();
        if actual_size > limit_bytes {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Err(CacheError::CapacityExceeded {
                required_bytes: actual_size,
                limit_bytes,
            });
        }

        state.index.entries.insert(
            key.to_string(),
            CacheEntry {
                size: actual_size,
                last_accessed_at_ms: now_ms(),
            },
        );
        if let Err(error) = self
            .evict_until_fits(&mut state, Some(key), 0, limit_bytes)
            .await
        {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Err(error);
        }
        self.persist(&mut state).await
    }

    pub async fn cancel_reservation(&self, key: &str) {
        if !is_diary_attachment_key(key) {
            return;
        }
        self.state.lock().await.reservations.remove(key);
    }

    /// 检查附件是否允许保留为本地缓存，不涉及总容量预留或淘汰。
    pub fn ensure_file_cacheable(&self, size: u64) -> Result<(), CacheError> {
        let limit_bytes = self.max_file_size_bytes();
        if size > limit_bytes {
            return Err(CacheError::AttachmentTooLarge {
                attachment_bytes: size,
                limit_bytes,
            });
        }
        Ok(())
    }

    /// 登记已经写入 LOS 的云端附件。附件大于上限时只移除本地副本。
    pub async fn register_existing(&self, key: &str) -> Result<bool, CacheError> {
        if !is_diary_attachment_key(key) {
            return Ok(true);
        }
        let Some(size) = self.los.get_size(key).await? else {
            return Ok(false);
        };
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        if self.ensure_file_cacheable(size).is_err() {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Ok(false);
        }
        let limit_bytes = self.limit_bytes();
        if size > limit_bytes {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Ok(false);
        }

        state.index.entries.insert(
            key.to_string(),
            CacheEntry {
                size,
                last_accessed_at_ms: now_ms(),
            },
        );
        if let Err(error) = self
            .evict_until_fits(&mut state, Some(key), 0, limit_bytes)
            .await
        {
            self.los.delete(key).await?;
            state.index.entries.remove(key);
            self.persist(&mut state).await?;
            return Err(error);
        }
        self.persist(&mut state).await?;
        Ok(true)
    }

    pub async fn touch(&self, key: &str) -> Result<(), CacheError> {
        if !is_diary_attachment_key(key) {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        let now = now_ms();
        let Some(entry) = state.index.entries.get_mut(key) else {
            return Ok(());
        };
        entry.last_accessed_at_ms = now;
        if now.saturating_sub(state.last_persisted_at_ms) >= ACCESS_PERSIST_INTERVAL_MS {
            self.persist(&mut state).await?;
        }
        Ok(())
    }

    pub async fn forget(&self, key: &str) -> Result<(), CacheError> {
        if !is_diary_attachment_key(key) {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        state.reservations.remove(key);
        if state.index.entries.remove(key).is_some() {
            self.persist(&mut state).await?;
        }
        Ok(())
    }

    pub async fn forget_diary(&self, diary_id: &str) -> Result<(), CacheError> {
        let mut state = self.state.lock().await;
        self.ensure_loaded(&mut state).await?;
        let prefix = format!("{diary_id}/");
        state
            .reservations
            .retain(|key, _| !key.starts_with(&prefix));
        let previous_len = state.index.entries.len();
        state
            .index
            .entries
            .retain(|key, _| !key.starts_with(&prefix));
        if state.index.entries.len() != previous_len {
            self.persist(&mut state).await?;
        }
        Ok(())
    }

    async fn ensure_loaded(&self, state: &mut CacheState) -> Result<(), CacheError> {
        if state.loaded {
            return Ok(());
        }

        state.index = self.load_index().await;
        if state.index.version != CACHE_INDEX_VERSION {
            state.index = CacheIndex::default();
        }

        let local_entries = self.los.get_all_entries().await?;
        let mut actual_keys = HashSet::new();
        for entry in local_entries
            .into_iter()
            .filter(|entry| is_diary_attachment_key(&entry.key))
        {
            actual_keys.insert(entry.key.clone());
            state
                .index
                .entries
                .entry(entry.key)
                .and_modify(|cached| cached.size = entry.size)
                .or_insert(CacheEntry {
                    size: entry.size,
                    last_accessed_at_ms: entry.modified_at_ms,
                });
        }
        state
            .index
            .entries
            .retain(|key, _| actual_keys.contains(key));
        state.index.version = CACHE_INDEX_VERSION;
        state.loaded = true;
        self.persist(state).await
    }

    async fn load_index(&self) -> CacheIndex {
        match tokio::fs::read(self.index_path()).await {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(index) => index,
                Err(error) => {
                    tauri_plugin_log::log::warn!("附件缓存索引损坏，将根据本地对象重建: {error}");
                    CacheIndex::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => CacheIndex::default(),
            Err(error) => {
                tauri_plugin_log::log::warn!("读取附件缓存索引失败，将重新扫描: {error}");
                CacheIndex::default()
            }
        }
    }

    async fn evict_until_fits(
        &self,
        state: &mut CacheState,
        protected_key: Option<&str>,
        additional_bytes: u64,
        limit_bytes: u64,
    ) -> Result<(), CacheError> {
        let replaced_bytes = if additional_bytes > 0 {
            protected_key
                .and_then(|key| state.index.entries.get(key))
                .map(|entry| entry.size)
                .unwrap_or_default()
        } else {
            0
        };
        let reserved_bytes = state
            .reservations
            .iter()
            .filter(|(key, _)| Some(key.as_str()) != protected_key)
            .map(|(_, size)| *size)
            .sum::<u64>();
        let mut used_bytes = index_size(&state.index).saturating_sub(replaced_bytes);
        if total_with_additional(used_bytes, reserved_bytes, additional_bytes) <= limit_bytes {
            return Ok(());
        }

        let reserved_keys = state.reservations.keys().cloned().collect::<HashSet<_>>();
        let mut candidates = state
            .index
            .entries
            .iter()
            .filter(|(key, _)| {
                Some(key.as_str()) != protected_key && !reserved_keys.contains(key.as_str())
            })
            .map(|(key, entry)| (entry.last_accessed_at_ms, key.clone(), entry.size))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));

        for (_, key, size) in candidates {
            if total_with_additional(used_bytes, reserved_bytes, additional_bytes) <= limit_bytes {
                break;
            }
            match self.los.delete(&key).await {
                Ok(()) => {
                    state.index.entries.remove(&key);
                    used_bytes = used_bytes.saturating_sub(size);
                    tauri_plugin_log::log::info!(
                        "附件缓存 LRU 淘汰: key={key}, size={size}, limit={limit_bytes}"
                    );
                }
                Err(error) => {
                    tauri_plugin_log::log::warn!(
                        "附件缓存暂时无法淘汰，继续尝试其他对象: key={key}, error={error}"
                    );
                }
            }
        }

        if total_with_additional(used_bytes, reserved_bytes, additional_bytes) > limit_bytes {
            return Err(CacheError::InsufficientEvictableCapacity {
                required_bytes: additional_bytes,
                limit_bytes,
            });
        }
        Ok(())
    }

    async fn evict_oversized(&self, state: &mut CacheState) -> Result<(), CacheError> {
        let max_file_size_bytes = self.max_file_size_bytes();
        let oversized = state
            .index
            .entries
            .iter()
            .filter(|(_, entry)| entry.size > max_file_size_bytes)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in oversized {
            self.los.delete(&key).await?;
            state.index.entries.remove(&key);
            tauri_plugin_log::log::info!(
                "附件缓存超过单文件上限并被移除: key={key}, limit={max_file_size_bytes}"
            );
        }
        Ok(())
    }

    async fn persist(&self, state: &mut CacheState) -> Result<(), CacheError> {
        let path = self.index_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(&state.index)
            .map_err(|error| CacheError::Metadata(error.to_string()))?;
        let temp_path = path.with_extension("json.tmp");
        tokio::fs::write(&temp_path, bytes).await?;
        remove_file_if_exists(&path).await?;
        tokio::fs::rename(temp_path, path).await?;
        state.last_persisted_at_ms = now_ms();
        Ok(())
    }

    fn limit_bytes(&self) -> u64 {
        self.app_config.current().attachment_cache_limit_bytes()
    }

    fn max_file_size_bytes(&self) -> u64 {
        self.app_config
            .current()
            .attachment_cache_max_file_size_bytes()
    }

    fn index_path(&self) -> PathBuf {
        self.los.root().join(CACHE_INDEX_FILENAME)
    }
}

fn stats(index: &CacheIndex, limit_bytes: u64, max_file_size_bytes: u64) -> AttachmentCacheStats {
    AttachmentCacheStats {
        cached_files: index.entries.len().try_into().unwrap_or(u32::MAX),
        cached_bytes: index_size(index),
        limit_bytes,
        max_file_size_bytes,
    }
}

fn index_size(index: &CacheIndex) -> u64 {
    index
        .entries
        .values()
        .fold(0_u64, |total, entry| total.saturating_add(entry.size))
}

fn total_with_additional(used_bytes: u64, reserved_bytes: u64, additional_bytes: u64) -> u64 {
    used_bytes
        .saturating_add(reserved_bytes)
        .saturating_add(additional_bytes)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

async fn remove_file_if_exists(path: &std::path::Path) -> Result<(), CacheError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;

    fn manager(root: &std::path::Path, limit_bytes: u64) -> AttachmentCacheManager {
        let los = LocalObjectStore::new(root.to_path_buf());
        let config =
            AppConfigStore::in_memory(AppConfig::with_attachment_cache_limit_bytes(limit_bytes));
        AttachmentCacheManager::new(los, config)
    }

    fn manager_with_limits(
        root: &std::path::Path,
        total_limit_bytes: u64,
        max_file_size_bytes: u64,
    ) -> AttachmentCacheManager {
        let los = LocalObjectStore::new(root.to_path_buf());
        let config = AppConfigStore::in_memory(AppConfig::with_attachment_cache_limit_bytes(
            total_limit_bytes,
        ));
        config
            .set_attachment_cache_max_file_size_bytes(max_file_size_bytes)
            .unwrap();
        AttachmentCacheManager::new(los, config)
    }

    async fn save(los: &LocalObjectStore, key: &str, size: usize) {
        los.save_bytes(key, &vec![7; size]).await.unwrap();
    }

    async fn write_index(root: &std::path::Path, entries: &[(&str, u64, u64)]) {
        let index = CacheIndex {
            version: CACHE_INDEX_VERSION,
            entries: entries
                .iter()
                .map(|(key, size, accessed)| {
                    (
                        (*key).to_string(),
                        CacheEntry {
                            size: *size,
                            last_accessed_at_ms: *accessed,
                        },
                    )
                })
                .collect(),
        };
        tokio::fs::write(
            root.join(CACHE_INDEX_FILENAME),
            serde_json::to_vec(&index).unwrap(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn activation_evicts_oldest_attachment_but_preserves_non_cache_objects() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-old", 4).await;
        save(&los, "100/att-new", 4).await;
        save(&los, "100/manifest.enc", 12).await;
        save(&los, "100/.attachment-transaction/att-backup", 6).await;
        write_index(temp.path(), &[("100/att-old", 4, 1), ("100/att-new", 4, 2)]).await;

        let stats = manager(temp.path(), 5).activate().await.unwrap();

        assert_eq!(stats.cached_files, 1);
        assert_eq!(stats.cached_bytes, 4);
        assert!(los.get("100/att-old").await.unwrap().is_none());
        assert!(los.get("100/att-new").await.unwrap().is_some());
        assert!(los.get("100/manifest.enc").await.unwrap().is_some());
        assert!(los
            .get("100/.attachment-transaction/att-backup")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn recent_access_changes_the_next_eviction_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-a", 4).await;
        save(&los, "100/att-b", 4).await;
        write_index(temp.path(), &[("100/att-a", 4, 1), ("100/att-b", 4, 2)]).await;
        let manager = manager(temp.path(), 8);
        manager.activate().await.unwrap();
        manager.touch("100/att-a").await.unwrap();

        manager.reserve("100/att-c", 4).await.unwrap();
        save(&los, "100/att-c", 4).await;
        manager.commit("100/att-c", 4).await.unwrap();

        assert!(los.get("100/att-a").await.unwrap().is_some());
        assert!(los.get("100/att-b").await.unwrap().is_none());
        assert!(los.get("100/att-c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn oversized_attachment_is_rejected_without_evicting_existing_cache() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-a", 4).await;
        let manager = manager(temp.path(), 5);
        manager.activate().await.unwrap();

        assert!(matches!(
            manager.reserve("100/too-large", 6).await,
            Err(CacheError::CapacityExceeded {
                required_bytes: 6,
                limit_bytes: 5
            })
        ));
        assert!(los.get("100/att-a").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn attachment_above_per_file_limit_is_not_cached() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        let manager = manager_with_limits(temp.path(), 20, 5);

        assert!(matches!(
            manager.reserve("100/att-large", 6).await,
            Err(CacheError::AttachmentTooLarge {
                attachment_bytes: 6,
                limit_bytes: 5
            })
        ));
        assert!(los.get("100/att-large").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn lowering_per_file_limit_removes_existing_oversized_cache() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-small", 4).await;
        save(&los, "100/att-large", 6).await;
        let manager = manager_with_limits(temp.path(), 20, 10);
        assert_eq!(manager.activate().await.unwrap().cached_files, 2);

        manager
            .app_config
            .set_attachment_cache_max_file_size_bytes(5)
            .unwrap();
        let stats = manager.enforce_limit().await.unwrap();

        assert_eq!(stats.cached_files, 1);
        assert!(los.get("100/att-small").await.unwrap().is_some());
        assert!(los.get("100/att-large").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_index_is_rebuilt_from_existing_attachments() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-a", 3).await;
        tokio::fs::write(temp.path().join(CACHE_INDEX_FILENAME), b"not-json")
            .await
            .unwrap();

        let stats = manager(temp.path(), 10).activate().await.unwrap();

        assert_eq!(stats.cached_files, 1);
        assert_eq!(stats.cached_bytes, 3);
        let rebuilt: CacheIndex = serde_json::from_slice(
            &tokio::fs::read(temp.path().join(CACHE_INDEX_FILENAME))
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(rebuilt.entries.contains_key("100/att-a"));
    }

    #[tokio::test]
    async fn oversized_uploaded_copy_is_removed_without_touching_other_cache() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-a", 4).await;
        let manager = manager(temp.path(), 5);
        manager.activate().await.unwrap();
        save(&los, "100/att-large", 6).await;

        assert!(!manager.register_existing("100/att-large").await.unwrap());
        assert!(los.get("100/att-a").await.unwrap().is_some());
        assert!(los.get("100/att-large").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deactivation_keeps_files_and_removes_only_the_index() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        save(&los, "100/att-a", 4).await;
        let manager = manager(temp.path(), 5);
        manager.activate().await.unwrap();

        manager.deactivate().await.unwrap();

        assert!(los.get("100/att-a").await.unwrap().is_some());
        assert!(!temp.path().join(CACHE_INDEX_FILENAME).exists());
    }
}

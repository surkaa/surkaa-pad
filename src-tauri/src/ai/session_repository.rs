use super::{
    append_and_compact_message, deserialize_session_message_block, deserialize_session_meta,
    load_all_compacted_messages, load_compacted_messages, AiMessageBlockError, AiMessageBlockStore,
    AiSessionDataError, AiSessionMessage, AiSessionMessageBlock, AiSessionMessagePayload,
    AiSessionMeta, CURRENT_AI_SESSION_VERSION,
};
use crate::app_object_store::{AppObjectStoreError, SharedAppObjectStore};
use crate::cryptos::{Crypto, CryptoError};
use crate::error::AppError;
use crate::object_locations::{StoredObject, StoredObjectCollection};
use crate::utils::id_generate::generate_descending_id;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::{Arc, Weak};
use std::time::Instant;
use tauri_plugin_log::log;
use thiserror::Error;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Debug, Error)]
pub enum AiSessionRepositoryError {
    #[error("AI 会话对象存储失败: {0}")]
    Store(#[from] AppObjectStoreError),
    #[error("AI 会话加解密失败: {0}")]
    Crypto(#[from] CryptoError),
    #[error("AI 会话数据无效: {0}")]
    Data(#[from] AiSessionDataError),
    #[error("AI 消息块操作失败: {0}")]
    MessageBlock(#[from] AiMessageBlockError),
    #[error("AI 会话 JSON 序列化失败: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("AI 会话 {0} 不存在")]
    SessionNotFound(String),
    #[error("AI 会话参数无效: {0}")]
    InvalidInput(String),
    #[error("AI 会话 {0} 正在生成回答")]
    SessionBusy(String),
    #[error("AI 会话已提交消息数量溢出")]
    CommittedMessageCountOverflow,
    #[error("AI 会话已提交 {committed} 条消息，但存储中只能恢复出 {actual} 条连续消息")]
    CommittedMessagesMissing { committed: u64, actual: u64 },
}

impl From<AiSessionRepositoryError> for AppError {
    fn from(error: AiSessionRepositoryError) -> Self {
        let error_type = match &error {
            AiSessionRepositoryError::SessionNotFound(_) => "ai_session_not_found",
            AiSessionRepositoryError::InvalidInput(_) => "ai_session_invalid_input",
            AiSessionRepositoryError::SessionBusy(_) => "ai_session_busy",
            _ => "ai_session",
        };
        Self {
            error_type: error_type.into(),
            message: error.to_string(),
        }
    }
}

/// AI 会话领域仓库：负责独立 JSON 文档、加解密、元数据提交和消息块合并。
///
/// `AppObjectStore` 只看到密文字节；同一 Repository 实例内，同一会话的修改会串行执行。
#[derive(Clone)]
pub struct AiSessionRepository {
    store: SharedAppObjectStore,
    crypto: Crypto,
    session_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    session_run_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
    reconciled_sessions: Arc<DashMap<String, ()>>,
}

impl AiSessionRepository {
    pub fn new(store: SharedAppObjectStore, crypto: Crypto) -> Self {
        Self {
            store,
            crypto,
            session_locks: Arc::new(DashMap::new()),
            session_run_locks: Arc::new(DashMap::new()),
            reconciled_sessions: Arc::new(DashMap::new()),
        }
    }

    /// 存储模式切换后强制各会话在下一次访问时重新核对实际消息块。
    pub fn invalidate_reconciliation(&self) {
        self.reconciled_sessions.clear();
    }

    /// 为一次完整模型问答占用会话，避免两个任务交叉追加用户和助手消息。
    pub fn try_begin_run(
        &self,
        session_id: &str,
    ) -> Result<OwnedMutexGuard<()>, AiSessionRepositoryError> {
        validate_session_id(session_id)?;
        named_lock(&self.session_run_locks, session_id)
            .try_lock_owned()
            .map_err(|_| AiSessionRepositoryError::SessionBusy(session_id.to_owned()))
    }

    pub async fn create_session(
        &self,
        title: String,
        model: String,
        created_at: i64,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
        let started_at = Instant::now();
        if title.trim().is_empty() {
            return Err(AiSessionRepositoryError::InvalidInput(
                "会话标题不能为空".into(),
            ));
        }
        if model.trim().is_empty() {
            return Err(AiSessionRepositoryError::InvalidInput(
                "会话模型不能为空".into(),
            ));
        }
        let meta = AiSessionMeta {
            version: CURRENT_AI_SESSION_VERSION,
            id: generate_descending_id(),
            title,
            ai_title: None,
            model,
            created_at,
            updated_at: created_at,
            committed_message_count: 0,
        };
        self.save_meta(&meta).await?;
        self.reconciled_sessions.insert(meta.id.clone(), ());
        log::info!(
            "[ai session timing] operation=create, session_id={}, total_ms={}",
            meta.id,
            started_at.elapsed().as_millis()
        );
        Ok(meta)
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AiSessionMeta>, AiSessionRepositoryError> {
        validate_session_id(session_id)?;
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let Some(meta) = self.load_meta(session_id).await? else {
            return Ok(None);
        };
        let (meta, _) = self.ensure_reconciled_locked(meta).await?;
        Ok(Some(meta))
    }

    /// 在同一个会话锁内读取并协调 meta 与全部消息，避免两次调用之间插入新消息。
    pub async fn load_session(
        &self,
        session_id: &str,
    ) -> Result<Option<(AiSessionMeta, Vec<AiSessionMessage>)>, AiSessionRepositoryError> {
        let started_at = Instant::now();
        validate_session_id(session_id)?;
        let lock = self.session_lock(session_id);
        let lock_started_at = Instant::now();
        let _guard = lock.lock().await;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        let Some(meta) = self.load_meta(session_id).await? else {
            log::info!(
                "[ai session timing] operation=load, session_id={}, found=false, lock_wait_ms={}, total_ms={}",
                session_id,
                lock_wait_ms,
                started_at.elapsed().as_millis()
            );
            return Ok(None);
        };
        let (meta, recovered_messages) = self.ensure_reconciled_locked(meta).await?;
        let messages = match recovered_messages {
            Some(messages) => messages,
            None => load_compacted_messages(self, session_id, meta.committed_message_count).await?,
        };
        log::info!(
            "[ai session timing] operation=load, session_id={}, found=true, messages={}, lock_wait_ms={}, total_ms={}",
            session_id,
            messages.len(),
            lock_wait_ms,
            started_at.elapsed().as_millis()
        );
        Ok(Some((meta, messages)))
    }

    pub async fn list_sessions(&self) -> Result<Vec<AiSessionMeta>, AiSessionRepositoryError> {
        let started_at = Instant::now();
        let objects = self
            .store
            .list(&StoredObjectCollection::AiSessionMetas)
            .await?;
        let mut sessions = Vec::with_capacity(objects.len());
        for object in objects {
            let StoredObject::AiSessionMeta { session_id } = object else {
                continue;
            };
            if let Some(meta) = self.load_meta(&session_id).await? {
                sessions.push(meta);
            }
        }
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        log::info!(
            "[ai session timing] operation=list, sessions={}, total_ms={}",
            sessions.len(),
            started_at.elapsed().as_millis()
        );
        Ok(sessions)
    }

    pub async fn append_message(
        &self,
        session_id: &str,
        created_at: i64,
        payload: AiSessionMessagePayload,
    ) -> Result<AiSessionMessage, AiSessionRepositoryError> {
        let started_at = Instant::now();
        validate_session_id(session_id)?;
        let role = match &payload {
            AiSessionMessagePayload::User { .. } => "user",
            AiSessionMessagePayload::Assistant { .. } => "assistant",
        };
        let lock = self.session_lock(session_id);
        let lock_started_at = Instant::now();
        let _guard = lock.lock().await;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        let meta = self.required_meta(session_id).await?;
        let (mut meta, _) = self.ensure_reconciled_locked(meta).await?;
        let message = AiSessionMessage {
            index: meta.committed_message_count,
            created_at,
            payload,
        };

        // 消息块先持久化并完成必要的逐级合并，最后才推进提交水位。
        // 因此 meta 始终不会声称存在尚未落盘的消息。
        if let Err(error) = append_and_compact_message(self, session_id, message.clone()).await {
            self.reconciled_sessions.remove(session_id);
            return Err(error.into());
        }
        meta.committed_message_count =
            meta.committed_message_count.checked_add(1).ok_or_else(|| {
                self.reconciled_sessions.remove(session_id);
                AiSessionRepositoryError::CommittedMessageCountOverflow
            })?;
        meta.updated_at = meta.updated_at.max(created_at);
        if let Err(error) = self.save_meta(&meta).await {
            // 消息块可能已经完整落盘；下次访问必须重新协调，而不能继续信任旧水位。
            self.reconciled_sessions.remove(session_id);
            return Err(error);
        }
        log::info!(
            "[ai session timing] operation=append, session_id={}, message_index={}, role={}, lock_wait_ms={}, total_ms={}",
            session_id,
            message.index,
            role,
            lock_wait_ms,
            started_at.elapsed().as_millis()
        );
        Ok(message)
    }

    pub async fn load_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<AiSessionMessage>, AiSessionRepositoryError> {
        let started_at = Instant::now();
        validate_session_id(session_id)?;
        let lock = self.session_lock(session_id);
        let lock_started_at = Instant::now();
        let _guard = lock.lock().await;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        let meta = self.required_meta(session_id).await?;
        let (meta, recovered_messages) = self.ensure_reconciled_locked(meta).await?;
        if let Some(messages) = recovered_messages {
            log::info!(
                "[ai session timing] operation=load_messages, session_id={}, messages={}, lock_wait_ms={}, total_ms={}",
                session_id,
                messages.len(),
                lock_wait_ms,
                started_at.elapsed().as_millis()
            );
            return Ok(messages);
        }
        let messages =
            load_compacted_messages(self, session_id, meta.committed_message_count).await?;
        log::info!(
            "[ai session timing] operation=load_messages, session_id={}, messages={}, lock_wait_ms={}, total_ms={}",
            session_id,
            messages.len(),
            lock_wait_ms,
            started_at.elapsed().as_millis()
        );
        Ok(messages)
    }

    pub async fn update_ai_title(
        &self,
        session_id: &str,
        ai_title: Option<String>,
        updated_at: i64,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
        validate_session_id(session_id)?;
        if ai_title
            .as_deref()
            .is_some_and(|title| title.trim().is_empty())
        {
            return Err(AiSessionRepositoryError::InvalidInput(
                "AI 生成的标题不能为空字符串".into(),
            ));
        }
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let meta = self.required_meta(session_id).await?;
        let (mut meta, _) = self.ensure_reconciled_locked(meta).await?;
        meta.ai_title = ai_title;
        meta.updated_at = meta.updated_at.max(updated_at);
        self.save_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn update_model(
        &self,
        session_id: &str,
        model: String,
        updated_at: i64,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
        validate_session_id(session_id)?;
        let model = model.trim();
        if model.is_empty() {
            return Err(AiSessionRepositoryError::InvalidInput(
                "会话模型不能为空".into(),
            ));
        }
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let meta = self.required_meta(session_id).await?;
        let (mut meta, _) = self.ensure_reconciled_locked(meta).await?;
        if meta.model == model {
            return Ok(meta);
        }
        meta.model = model.to_owned();
        meta.updated_at = meta.updated_at.max(updated_at);
        self.save_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), AiSessionRepositoryError> {
        let started_at = Instant::now();
        validate_session_id(session_id)?;
        let lock = self.session_lock(session_id);
        let lock_started_at = Instant::now();
        let _guard = lock.lock().await;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        let blocks = self
            .store
            .list(&StoredObjectCollection::AiSessionMessageBlocks {
                session_id: session_id.to_owned(),
            })
            .await?;
        let block_count = blocks.len();
        for block in blocks {
            self.store.delete(&block).await?;
        }
        // meta 是会话可见性的提交标志；消息清理全部成功后才删除它。
        self.store
            .delete(&StoredObject::AiSessionMeta {
                session_id: session_id.to_owned(),
            })
            .await?;
        self.reconciled_sessions.remove(session_id);
        log::info!(
            "[ai session timing] operation=delete, session_id={}, blocks={}, lock_wait_ms={}, total_ms={}",
            session_id,
            block_count,
            lock_wait_ms,
            started_at.elapsed().as_millis()
        );
        Ok(())
    }

    fn session_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        named_lock(&self.session_locks, session_id)
    }

    async fn required_meta(
        &self,
        session_id: &str,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
        self.load_meta(session_id)
            .await?
            .ok_or_else(|| AiSessionRepositoryError::SessionNotFound(session_id.to_owned()))
    }

    /// 首次访问会话或上次写入失败后，根据实际消息块校准 meta 的提交水位。
    ///
    /// 物理消息多于提交水位，说明消息块已完整写入但 meta 提交中断，此时向前推进
    /// 水位；物理消息少于提交水位则意味着已确认的数据丢失，必须报错而不能回退。
    async fn ensure_reconciled_locked(
        &self,
        mut meta: AiSessionMeta,
    ) -> Result<(AiSessionMeta, Option<Vec<AiSessionMessage>>), AiSessionRepositoryError> {
        if self.reconciled_sessions.contains_key(&meta.id) {
            return Ok((meta, None));
        }

        let messages = load_all_compacted_messages(self, &meta.id).await?;
        let actual = u64::try_from(messages.len())
            .map_err(|_| AiSessionRepositoryError::CommittedMessageCountOverflow)?;
        if actual < meta.committed_message_count {
            return Err(AiSessionRepositoryError::CommittedMessagesMissing {
                committed: meta.committed_message_count,
                actual,
            });
        }
        if actual > meta.committed_message_count {
            meta.committed_message_count = actual;
            if let Some(last_message) = messages.last() {
                meta.updated_at = meta.updated_at.max(last_message.created_at);
            }
            self.save_meta(&meta).await?;
        }
        self.reconciled_sessions.insert(meta.id.clone(), ());
        Ok((meta, Some(messages)))
    }

    async fn load_meta(
        &self,
        session_id: &str,
    ) -> Result<Option<AiSessionMeta>, AiSessionRepositoryError> {
        let object = StoredObject::AiSessionMeta {
            session_id: session_id.to_owned(),
        };
        let Some(encrypted) = self.store.load_bytes(&object).await? else {
            return Ok(None);
        };
        let plaintext = self.crypto.decrypt(&encrypted)?;
        Ok(Some(deserialize_session_meta(session_id, &plaintext)?))
    }

    async fn save_meta(&self, meta: &AiSessionMeta) -> Result<(), AiSessionRepositoryError> {
        let plaintext = serde_json::to_vec(meta)?;
        // 写入前也走一次统一校验，避免 Repository 自己生成非法文档。
        deserialize_session_meta(&meta.id, &plaintext)?;
        let encrypted = self.crypto.encrypt(&plaintext)?;
        self.store
            .save_bytes(
                &StoredObject::AiSessionMeta {
                    session_id: meta.id.clone(),
                },
                &encrypted,
            )
            .await?;
        Ok(())
    }

    fn block_object(session_id: &str, level: u32, block_id: u64) -> StoredObject {
        StoredObject::AiSessionMessageBlock {
            session_id: session_id.to_owned(),
            level,
            block_id,
        }
    }
}

#[async_trait]
impl AiMessageBlockStore for AiSessionRepository {
    async fn load_block(
        &self,
        session_id: &str,
        level: u32,
        block_id: u64,
    ) -> Result<Option<AiSessionMessageBlock>, AiMessageBlockError> {
        let object = Self::block_object(session_id, level, block_id);
        let encrypted = self
            .store
            .load_bytes(&object)
            .await
            .map_err(block_storage_error)?;
        let Some(encrypted) = encrypted else {
            return Ok(None);
        };
        let plaintext = self
            .crypto
            .decrypt(&encrypted)
            .map_err(block_storage_error)?;
        let block = deserialize_session_message_block(session_id, level, block_id, &plaintext)
            .map_err(|error| AiMessageBlockError::InvalidBlock(error.to_string()))?;
        Ok(Some(block))
    }

    async fn save_block(&self, block: &AiSessionMessageBlock) -> Result<(), AiMessageBlockError> {
        let plaintext = serde_json::to_vec(block).map_err(block_storage_error)?;
        deserialize_session_message_block(
            &block.session_id,
            block.level,
            block.block_id,
            &plaintext,
        )
        .map_err(|error| AiMessageBlockError::InvalidBlock(error.to_string()))?;
        let encrypted = self
            .crypto
            .encrypt(&plaintext)
            .map_err(block_storage_error)?;
        self.store
            .save_bytes(
                &Self::block_object(&block.session_id, block.level, block.block_id),
                &encrypted,
            )
            .await
            .map_err(block_storage_error)
    }

    async fn delete_block(
        &self,
        session_id: &str,
        level: u32,
        block_id: u64,
    ) -> Result<(), AiMessageBlockError> {
        self.store
            .delete(&Self::block_object(session_id, level, block_id))
            .await
            .map_err(block_storage_error)
    }

    async fn list_blocks(
        &self,
        session_id: &str,
    ) -> Result<Vec<AiSessionMessageBlock>, AiMessageBlockError> {
        let objects = self
            .store
            .list(&StoredObjectCollection::AiSessionMessageBlocks {
                session_id: session_id.to_owned(),
            })
            .await
            .map_err(block_storage_error)?;
        let mut blocks = Vec::with_capacity(objects.len());
        for object in objects {
            let StoredObject::AiSessionMessageBlock {
                session_id,
                level,
                block_id,
            } = object
            else {
                continue;
            };
            if let Some(block) = self.load_block(&session_id, level, block_id).await? {
                blocks.push(block);
            }
        }
        Ok(blocks)
    }
}

fn block_storage_error(error: impl std::fmt::Display) -> AiMessageBlockError {
    AiMessageBlockError::Storage(error.to_string())
}

fn validate_session_id(session_id: &str) -> Result<(), AiSessionRepositoryError> {
    if session_id.is_empty() || !session_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AiSessionRepositoryError::InvalidInput(
            "会话 ID 必须是非空数字".into(),
        ));
    }
    Ok(())
}

fn named_lock(locks: &DashMap<String, Weak<Mutex<()>>>, session_id: &str) -> Arc<Mutex<()>> {
    let mut entry = locks.entry(session_id.to_owned()).or_default();
    if let Some(lock) = entry.upgrade() {
        lock
    } else {
        let lock = Arc::new(Mutex::new(()));
        *entry = Arc::downgrade(&lock);
        lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_object_store::{AppObjectStore, AppObjectStoreError, LocalAppObjectStore};
    use crate::caches::{CacheError, LocalObjectStore};
    use crate::object_locations::ObjectLocations;
    use std::sync::Mutex as StdMutex;

    struct FaultInjectingStore {
        inner: LocalAppObjectStore,
        save_failure: StdMutex<Option<StoredObject>>,
        delete_failure: StdMutex<Option<StoredObject>>,
    }

    impl FaultInjectingStore {
        fn new(inner: LocalAppObjectStore) -> Self {
            Self {
                inner,
                save_failure: StdMutex::new(None),
                delete_failure: StdMutex::new(None),
            }
        }

        fn fail_save_once(&self, object: StoredObject) {
            *self.save_failure.lock().unwrap() = Some(object);
        }

        fn fail_delete_once(&self, object: StoredObject) {
            *self.delete_failure.lock().unwrap() = Some(object);
        }

        fn take_matching_failure(
            failure: &StdMutex<Option<StoredObject>>,
            object: &StoredObject,
        ) -> bool {
            let mut failure = failure.lock().unwrap();
            if failure.as_ref() == Some(object) {
                failure.take();
                true
            } else {
                false
            }
        }
    }

    #[async_trait]
    impl AppObjectStore for FaultInjectingStore {
        async fn load_bytes(
            &self,
            object: &StoredObject,
        ) -> Result<Option<Vec<u8>>, AppObjectStoreError> {
            self.inner.load_bytes(object).await
        }

        async fn save_bytes(
            &self,
            object: &StoredObject,
            data: &[u8],
        ) -> Result<(), AppObjectStoreError> {
            if Self::take_matching_failure(&self.save_failure, object) {
                return Err(CacheError::Metadata("模拟对象写入失败".into()).into());
            }
            self.inner.save_bytes(object, data).await
        }

        async fn delete(&self, object: &StoredObject) -> Result<(), AppObjectStoreError> {
            if Self::take_matching_failure(&self.delete_failure, object) {
                return Err(CacheError::Metadata("模拟消息块删除失败".into()).into());
            }
            self.inner.delete(object).await
        }

        async fn list(
            &self,
            collection: &StoredObjectCollection,
        ) -> Result<Vec<StoredObject>, AppObjectStoreError> {
            self.inner.list(collection).await
        }
    }

    fn crypto() -> Crypto {
        let crypto = Crypto::new();
        crypto
            .derive_dek(
                "repository-test-password".into(),
                "c2Vzc2lvbi1yZXBvc2l0b3J5LXRlc3Qtc2FsdA",
            )
            .unwrap();
        crypto
    }

    fn user_message(index: u64) -> AiSessionMessagePayload {
        AiSessionMessagePayload::User {
            content: format!("消息 {index}"),
            timezone_offset_minutes: None,
        }
    }

    #[tokio::test]
    async fn persists_encrypted_metadata_and_compacted_messages_in_real_los() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store: SharedAppObjectStore = Arc::new(LocalAppObjectStore::new(local.clone()));
        let repository = AiSessionRepository::new(store, crypto());
        let meta = repository
            .create_session("第一条消息".into(), "deepseek-chat".into(), 100)
            .await
            .unwrap();

        for index in 0..25 {
            repository
                .append_message(&meta.id, 101 + index as i64, user_message(index))
                .await
                .unwrap();
        }

        let entries = local
            .get_entries_with_prefix(&ObjectLocations::ai_session_prefix(&meta.id))
            .await
            .unwrap();
        let mut keys = entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect::<Vec<_>>();
        keys.sort();
        let mut expected = vec![ObjectLocations::ai_session_meta(&meta.id)];
        expected.extend(
            (20..25).map(|index| ObjectLocations::ai_session_message_block(&meta.id, 0, index)),
        );
        expected.extend(
            (0..2).map(|block_id| ObjectLocations::ai_session_message_block(&meta.id, 1, block_id)),
        );
        expected.sort();
        assert_eq!(keys, expected);

        for entry in entries {
            let ciphertext = local.get_data(&entry.key).await.unwrap();
            assert!(serde_json::from_slice::<serde_json::Value>(&ciphertext).is_err());
        }
        let loaded = repository.load_messages(&meta.id).await.unwrap();
        assert_eq!(loaded.len(), 25);
        for (index, message) in loaded.into_iter().enumerate() {
            assert_eq!(message.index, index as u64);
            assert_eq!(message.payload, user_message(index as u64));
        }
        assert_eq!(
            repository
                .get_session(&meta.id)
                .await
                .unwrap()
                .unwrap()
                .committed_message_count,
            25
        );
        let (loaded_meta, loaded_messages) =
            repository.load_session(&meta.id).await.unwrap().unwrap();
        assert_eq!(loaded_meta.committed_message_count, 25);
        assert_eq!(loaded_messages.len(), 25);
    }

    #[tokio::test]
    async fn lists_by_update_time_and_updates_session_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let store: SharedAppObjectStore = Arc::new(LocalAppObjectStore::new(
            LocalObjectStore::new(temp.path().to_path_buf()),
        ));
        let repository = AiSessionRepository::new(store, crypto());
        let first = repository
            .create_session("first".into(), "model".into(), 100)
            .await
            .unwrap();
        let second = repository
            .create_session("second".into(), "model".into(), 200)
            .await
            .unwrap();
        let updated = repository
            .update_ai_title(&first.id, Some("AI 标题".into()), 300)
            .await
            .unwrap();

        assert_eq!(updated.ai_title.as_deref(), Some("AI 标题"));
        let updated = repository
            .update_model(&first.id, "new-model".into(), 400)
            .await
            .unwrap();
        assert_eq!(updated.model, "new-model");
        assert_eq!(
            repository
                .list_sessions()
                .await
                .unwrap()
                .into_iter()
                .map(|meta| meta.id)
                .collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
    }

    #[tokio::test]
    async fn deleting_a_session_removes_blocks_before_its_visibility_marker() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = Arc::new(FaultInjectingStore::new(LocalAppObjectStore::new(
            local.clone(),
        )));
        let repository = AiSessionRepository::new(store.clone(), crypto());
        let meta = repository
            .create_session("session".into(), "model".into(), 100)
            .await
            .unwrap();
        for index in 0..12 {
            repository
                .append_message(&meta.id, 101 + index as i64, user_message(index))
                .await
                .unwrap();
        }

        store.fail_delete_once(StoredObject::AiSessionMessageBlock {
            session_id: meta.id.clone(),
            level: 1,
            block_id: 0,
        });
        assert!(repository.delete_session(&meta.id).await.is_err());
        assert!(repository.load_meta(&meta.id).await.unwrap().is_some());

        repository.delete_session(&meta.id).await.unwrap();

        assert!(repository.get_session(&meta.id).await.unwrap().is_none());
        assert!(local
            .get_entries_with_prefix(&ObjectLocations::ai_session_prefix(&meta.id))
            .await
            .unwrap()
            .is_empty());
        // 删除是幂等的，便于中断后重试清理。
        repository.delete_session(&meta.id).await.unwrap();
    }

    #[tokio::test]
    async fn reconciles_a_compacted_message_when_meta_commit_was_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp.path().to_path_buf());
        let store = Arc::new(FaultInjectingStore::new(LocalAppObjectStore::new(
            local.clone(),
        )));
        let repository = AiSessionRepository::new(store.clone(), crypto());
        let meta = repository
            .create_session("session".into(), "model".into(), 100)
            .await
            .unwrap();
        for index in 0..9 {
            repository
                .append_message(&meta.id, 101 + index as i64, user_message(index))
                .await
                .unwrap();
        }

        store.fail_save_once(StoredObject::AiSessionMeta {
            session_id: meta.id.clone(),
        });
        assert!(repository
            .append_message(&meta.id, 110, user_message(9))
            .await
            .is_err());
        assert_eq!(
            repository
                .load_meta(&meta.id)
                .await
                .unwrap()
                .unwrap()
                .committed_message_count,
            9
        );
        assert_eq!(
            store
                .list(&StoredObjectCollection::AiSessionMessageBlocks {
                    session_id: meta.id.clone(),
                })
                .await
                .unwrap(),
            vec![StoredObject::AiSessionMessageBlock {
                session_id: meta.id.clone(),
                level: 1,
                block_id: 0,
            }]
        );

        // 模拟应用重启：进程内协调标记丢失，只能根据加密消息块恢复提交水位。
        let restarted = AiSessionRepository::new(store, crypto());
        let recovered = restarted.get_session(&meta.id).await.unwrap().unwrap();
        assert_eq!(recovered.committed_message_count, 10);
        assert_eq!(recovered.updated_at, 110);
        assert_eq!(restarted.load_messages(&meta.id).await.unwrap().len(), 10);

        let next = restarted
            .append_message(&meta.id, 111, user_message(10))
            .await
            .unwrap();
        assert_eq!(next.index, 10);
        assert_eq!(
            restarted
                .get_session(&meta.id)
                .await
                .unwrap()
                .unwrap()
                .committed_message_count,
            11
        );
    }

    #[tokio::test]
    async fn never_silently_rolls_back_a_committed_message_watermark() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(FaultInjectingStore::new(LocalAppObjectStore::new(
            LocalObjectStore::new(temp.path().to_path_buf()),
        )));
        let repository = AiSessionRepository::new(store.clone(), crypto());
        let meta = repository
            .create_session("session".into(), "model".into(), 100)
            .await
            .unwrap();
        repository
            .append_message(&meta.id, 101, user_message(0))
            .await
            .unwrap();
        store
            .inner
            .delete(&StoredObject::AiSessionMessageBlock {
                session_id: meta.id.clone(),
                level: 0,
                block_id: 0,
            })
            .await
            .unwrap();

        let restarted = AiSessionRepository::new(store, crypto());
        assert!(matches!(
            restarted.get_session(&meta.id).await,
            Err(AiSessionRepositoryError::CommittedMessagesMissing {
                committed: 1,
                actual: 0,
            })
        ));
    }

    #[tokio::test]
    async fn rejects_invalid_inputs_and_missing_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let store: SharedAppObjectStore = Arc::new(LocalAppObjectStore::new(
            LocalObjectStore::new(temp.path().to_path_buf()),
        ));
        let repository = AiSessionRepository::new(store, crypto());

        assert!(matches!(
            repository
                .create_session(" ".into(), "model".into(), 0)
                .await,
            Err(AiSessionRepositoryError::InvalidInput(_))
        ));
        assert!(matches!(
            repository
                .append_message("123", 0, user_message(0))
                .await,
            Err(AiSessionRepositoryError::SessionNotFound(id)) if id == "123"
        ));
        assert!(matches!(
            repository.get_session("../123").await,
            Err(AiSessionRepositoryError::InvalidInput(_))
        ));
        let session = repository
            .create_session("title".into(), "model".into(), 1)
            .await
            .unwrap();
        assert!(matches!(
            repository.update_model(&session.id, " ".into(), 2).await,
            Err(AiSessionRepositoryError::InvalidInput(_))
        ));

        let invalid: AppError = AiSessionRepositoryError::InvalidInput("bad".into()).into();
        assert_eq!(invalid.error_type, "ai_session_invalid_input");
        let missing: AppError = AiSessionRepositoryError::SessionNotFound("123".into()).into();
        assert_eq!(missing.error_type, "ai_session_not_found");
    }

    #[test]
    fn prevents_two_agent_runs_from_using_the_same_session() {
        let temp = tempfile::tempdir().unwrap();
        let store: SharedAppObjectStore = Arc::new(LocalAppObjectStore::new(
            LocalObjectStore::new(temp.path().to_path_buf()),
        ));
        let repository = AiSessionRepository::new(store, crypto());

        let first = repository.try_begin_run("123").unwrap();
        assert!(matches!(
            repository.try_begin_run("123"),
            Err(AiSessionRepositoryError::SessionBusy(id)) if id == "123"
        ));
        // 不同会话之间仍可并发运行。
        let other = repository.try_begin_run("456").unwrap();
        drop(first);
        assert!(repository.try_begin_run("123").is_ok());
        drop(other);
    }
}

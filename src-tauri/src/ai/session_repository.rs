use super::{
    append_and_compact_message, deserialize_session_message_block, deserialize_session_meta,
    load_compacted_messages, AiMessageBlockError, AiMessageBlockStore, AiSessionDataError,
    AiSessionMessage, AiSessionMessageBlock, AiSessionMessagePayload, AiSessionMeta,
    CURRENT_AI_SESSION_VERSION,
};
use crate::app_object_store::{AppObjectStoreError, SharedAppObjectStore};
use crate::cryptos::{Crypto, CryptoError};
use crate::object_locations::{StoredObject, StoredObjectCollection};
use crate::utils::id_generate::generate_descending_id;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::{Arc, Weak};
use thiserror::Error;
use tokio::sync::Mutex;

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
    #[error("AI 会话消息数量已溢出")]
    MessageCountOverflow,
}

/// AI 会话领域仓库：负责独立 JSON 文档、加解密、元数据提交和消息块合并。
///
/// `AppObjectStore` 只看到密文字节；同一 Repository 实例内，同一会话的修改会串行执行。
#[derive(Clone)]
pub struct AiSessionRepository {
    store: SharedAppObjectStore,
    crypto: Crypto,
    session_locks: Arc<DashMap<String, Weak<Mutex<()>>>>,
}

impl AiSessionRepository {
    pub fn new(store: SharedAppObjectStore, crypto: Crypto) -> Self {
        Self {
            store,
            crypto,
            session_locks: Arc::new(DashMap::new()),
        }
    }

    pub async fn create_session(
        &self,
        title: String,
        model: String,
        created_at: i64,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
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
            message_count: 0,
        };
        self.save_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AiSessionMeta>, AiSessionRepositoryError> {
        self.load_meta(session_id).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<AiSessionMeta>, AiSessionRepositoryError> {
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
        Ok(sessions)
    }

    pub async fn append_message(
        &self,
        session_id: &str,
        created_at: i64,
        payload: AiSessionMessagePayload,
    ) -> Result<AiSessionMessage, AiSessionRepositoryError> {
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let mut meta = self.required_meta(session_id).await?;
        let message = AiSessionMessage {
            index: meta.message_count,
            created_at,
            payload,
        };

        // 消息块先持久化并完成必要的逐级合并，最后才增加 meta.messageCount。
        // 因此 meta 始终不会声称存在尚未落盘的消息。
        append_and_compact_message(self, session_id, message.clone()).await?;
        meta.message_count = meta
            .message_count
            .checked_add(1)
            .ok_or(AiSessionRepositoryError::MessageCountOverflow)?;
        meta.updated_at = meta.updated_at.max(created_at);
        self.save_meta(&meta).await?;
        Ok(message)
    }

    pub async fn load_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<AiSessionMessage>, AiSessionRepositoryError> {
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let meta = self.required_meta(session_id).await?;
        Ok(load_compacted_messages(self, session_id, meta.message_count).await?)
    }

    pub async fn update_ai_title(
        &self,
        session_id: &str,
        ai_title: Option<String>,
        updated_at: i64,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
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
        let mut meta = self.required_meta(session_id).await?;
        meta.ai_title = ai_title;
        meta.updated_at = meta.updated_at.max(updated_at);
        self.save_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), AiSessionRepositoryError> {
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        let blocks = self
            .store
            .list(&StoredObjectCollection::AiSessionMessageBlocks {
                session_id: session_id.to_owned(),
            })
            .await?;
        for block in blocks {
            self.store.delete(&block).await?;
        }
        // meta 是会话可见性的提交标志；消息清理全部成功后才删除它。
        self.store
            .delete(&StoredObject::AiSessionMeta {
                session_id: session_id.to_owned(),
            })
            .await?;
        Ok(())
    }

    fn session_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut entry = self.session_locks.entry(session_id.to_owned()).or_default();
        if let Some(lock) = entry.upgrade() {
            lock
        } else {
            let lock = Arc::new(Mutex::new(()));
            *entry = Arc::downgrade(&lock);
            lock
        }
    }

    async fn required_meta(
        &self,
        session_id: &str,
    ) -> Result<AiSessionMeta, AiSessionRepositoryError> {
        self.load_meta(session_id)
            .await?
            .ok_or_else(|| AiSessionRepositoryError::SessionNotFound(session_id.to_owned()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_object_store::{AppObjectStore, AppObjectStoreError, LocalAppObjectStore};
    use crate::caches::{CacheError, LocalObjectStore};
    use crate::object_locations::ObjectLocations;
    use std::sync::Mutex as StdMutex;

    struct FailDeleteOnceStore {
        inner: LocalAppObjectStore,
        failure: StdMutex<Option<StoredObject>>,
    }

    impl FailDeleteOnceStore {
        fn new(inner: LocalAppObjectStore) -> Self {
            Self {
                inner,
                failure: StdMutex::new(None),
            }
        }

        fn fail_once(&self, object: StoredObject) {
            *self.failure.lock().unwrap() = Some(object);
        }
    }

    #[async_trait]
    impl AppObjectStore for FailDeleteOnceStore {
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
            self.inner.save_bytes(object, data).await
        }

        async fn delete(&self, object: &StoredObject) -> Result<(), AppObjectStoreError> {
            let should_fail = {
                let mut failure = self.failure.lock().unwrap();
                if failure.as_ref() == Some(object) {
                    failure.take();
                    true
                } else {
                    false
                }
            };
            if should_fail {
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
                .message_count,
            25
        );
    }

    #[tokio::test]
    async fn lists_by_update_time_and_updates_ai_title() {
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
        let store = Arc::new(FailDeleteOnceStore::new(LocalAppObjectStore::new(
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

        store.fail_once(StoredObject::AiSessionMessageBlock {
            session_id: meta.id.clone(),
            level: 1,
            block_id: 0,
        });
        assert!(repository.delete_session(&meta.id).await.is_err());
        assert!(repository.get_session(&meta.id).await.unwrap().is_some());

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
    }
}

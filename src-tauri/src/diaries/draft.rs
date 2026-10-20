//! 本机正文草稿：认证加密、同目录原子替换、按回执版本清理；不属于 LOS 缓存。
use super::{DiaryContent, DiaryError};
use crate::{cryptos::Crypto, object_locations::ObjectLocations};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{path::PathBuf, sync::Arc};
use tokio::{io::AsyncWriteExt, sync::Mutex};

const MAX_DRAFT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryDraft {
    version: u32,
    pub diary_id: String,
    pub revision: String,
    #[specta(type = f64)]
    pub saved_at: i64,
    pub content: DiaryContent,
}

#[derive(Clone)]
pub(crate) struct DiaryDraftRepository {
    root: PathBuf,
    crypto: Crypto,
    gate: Arc<Mutex<()>>,
}

impl DiaryDraftRepository {
    pub fn new(root: PathBuf, crypto: Crypto) -> Self {
        Self {
            root,
            crypto,
            gate: Arc::new(Mutex::new(())),
        }
    }
    fn path(&self, id: &str) -> Result<PathBuf, DiaryError> {
        if ObjectLocations::diary_edit_lease_id(&ObjectLocations::diary_edit_lease(id)) != Some(id)
        {
            return Err(DiaryError::InvalidManifest("无效的草稿日记 ID".into()));
        }
        Ok(self.root.join(format!("{id}.enc")))
    }
    async fn read_locked(&self, id: &str) -> Result<Option<DiaryDraft>, DiaryError> {
        let path = self.path(id)?;
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if metadata.len() > (MAX_DRAFT_BYTES + 64) as u64 {
            return Err(DiaryError::InvalidManifest(
                "本机草稿超过安全读取上限，已保留文件".into(),
            ));
        }
        let bytes = tokio::fs::read(path).await?;
        let plaintext = zeroize::Zeroizing::new(self.crypto.decrypt(&bytes)?);
        let draft: DiaryDraft = serde_json::from_slice(&plaintext)?;
        if draft.version != 1 || draft.diary_id != id {
            return Err(DiaryError::InvalidManifest(
                "无法识别本机草稿，已保留文件".into(),
            ));
        }
        Ok(Some(draft))
    }
    pub async fn read(&self, id: &str) -> Result<Option<DiaryDraft>, DiaryError> {
        let _guard = self.gate.lock().await;
        self.read_locked(id).await
    }
    pub async fn save(&self, id: &str, content: DiaryContent) -> Result<String, DiaryError> {
        let _guard = self.gate.lock().await;
        let path = self.path(id)?;
        let mut revision = [0; 16];
        getrandom::fill(&mut revision).map_err(|e| DiaryError::InvalidManifest(e.to_string()))?;
        let draft = DiaryDraft {
            version: 1,
            diary_id: id.into(),
            revision: hex::encode(revision),
            saved_at: chrono::Utc::now().timestamp_millis(),
            content,
        };
        let plaintext = zeroize::Zeroizing::new(serde_json::to_vec(&draft)?);
        if plaintext.len() > MAX_DRAFT_BYTES {
            return Err(DiaryError::InvalidManifest(
                "草稿超过 16 MiB，未覆盖原有草稿".into(),
            ));
        }
        let bytes = self.crypto.encrypt(&plaintext)?;
        tokio::fs::create_dir_all(&self.root).await?;
        let temp = path.with_extension("enc.tmp");
        let mut file = tokio::fs::File::create(&temp).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        // Windows 的 rename 使用替换语义；绝不先删旧草稿。中断的 .tmp 不影响旧文件。
        tokio::fs::rename(&temp, &path).await?;
        #[cfg(unix)]
        tokio::fs::File::open(&self.root).await?.sync_all().await?;
        Ok(draft.revision)
    }
    pub async fn clear_if_revision(&self, id: &str, revision: &str) -> Result<bool, DiaryError> {
        let _guard = self.gate.lock().await;
        if self
            .read_locked(id)
            .await?
            .is_none_or(|draft| draft.revision != revision)
        {
            return Ok(false);
        }
        tokio::fs::remove_file(self.path(id)?).await?;
        Ok(true)
    }
    /// 改密前不能遗留依赖旧密钥的本机草稿；损坏/未知文件也不能静默忽略。
    pub async fn has_pending(&self) -> Result<bool, DiaryError> {
        Ok(!self.list_ids().await?.is_empty())
    }

    pub async fn list_ids(&self) -> Result<Vec<String>, DiaryError> {
        let _guard = self.gate.lock().await;
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(error.into()),
        };
        let mut ids = vec![];
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().is_some_and(|ext| ext == "enc") {
                let id = entry
                    .path()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string();
                self.path(&id)?;
                ids.push(id);
            }
        }
        ids.sort();
        Ok(ids)
    }
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;

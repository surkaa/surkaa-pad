//! 协作式短期占用，不声称 OSS 支持目标 CAS。流程及竞态边界见 docs/diary-edit-lease.md。
use super::DiaryError;
use crate::{object::OssClient, object_locations::ObjectLocations};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedMutexGuard};

const LEASE_MILLIS: i64 = 120_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LeaseRecord {
    version: u32,
    diary_id: String,
    token: String,
    expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiaryEditAccess {
    pub session_id: Option<String>,
    pub read_only: bool,
    pub can_take_over: bool,
    pub reason: Option<String>,
}

impl DiaryEditAccess {
    pub fn local() -> Self {
        Self {
            session_id: None,
            read_only: false,
            can_take_over: false,
            reason: None,
        }
    }
    pub fn read_only(reason: impl ToString, can_take_over: bool) -> Self {
        Self {
            session_id: None,
            read_only: true,
            can_take_over,
            reason: Some(reason.to_string()),
        }
    }
}

#[async_trait]
pub(super) trait LeaseStorage: Send + Sync {
    /// 时间来自本次服务端响应，不依赖设备的 UTC 时钟。
    async fn read(&self, id: &str) -> Result<(Option<LeaseRecord>, i64), DiaryError>;
    async fn write(&self, id: &str, record: &LeaseRecord) -> Result<(), DiaryError>;
    async fn manifest_etag(&self, id: &str) -> Result<String, DiaryError>;
}

struct OssLeaseStorage(OssClient);

#[async_trait]
impl LeaseStorage for OssLeaseStorage {
    async fn read(&self, id: &str) -> Result<(Option<LeaseRecord>, i64), DiaryError> {
        let key = ObjectLocations::diary_edit_lease(id);
        if let Some(meta) = self.0.get_metadata_if_exists(&key).await? {
            if meta.content_length.is_none_or(|size| size > 4096) {
                return Err(unavailable("占用文件过大或缺少大小，已改为只读"));
            }
            let (bytes, _, now) = self.0.download_snapshot(&key).await?;
            if bytes.len() > 4096 {
                return Err(unavailable("占用文件过大"));
            }
            let record: LeaseRecord = serde_json::from_slice(&bytes)?;
            if record.version != 1 || record.diary_id != id || record.token.len() != 32 {
                return Err(unavailable("无法识别云端占用记录，已改为只读"));
            }
            return Ok((Some(record), now));
        }
        // 无占用时也使用 OSS 时钟；同时确认日记确实存在。
        let (_, _, now) = self
            .0
            .download_snapshot(&ObjectLocations::diary_manifest(id))
            .await?;
        Ok((None, now))
    }
    async fn write(&self, id: &str, record: &LeaseRecord) -> Result<(), DiaryError> {
        self.0
            .upload_bytes(
                &ObjectLocations::diary_edit_lease(id),
                &serde_json::to_vec(record)?,
            )
            .await?;
        Ok(())
    }
    async fn manifest_etag(&self, id: &str) -> Result<String, DiaryError> {
        self.0
            .get_metadata(&ObjectLocations::diary_manifest(id))
            .await?
            .etag
            .filter(|etag| !etag.is_empty())
            .ok_or_else(|| unavailable("日记已被删除或缺少版本标识"))
    }
}

fn unavailable(message: impl ToString) -> DiaryError {
    DiaryError::EditUnavailable(message.to_string())
}

struct EditSession {
    token: String,
    valid: AtomicBool,
    data: Arc<Mutex<SessionData>>,
}

#[derive(Debug)]
pub(crate) struct SessionData {
    pub baseline: String,
    confirmed: Instant,
}

#[derive(Clone)]
pub(crate) struct DiaryEditManager {
    storage: Arc<dyn LeaseStorage>,
    sessions: Arc<DashMap<String, Arc<EditSession>>>,
}

/// 在操作开始时捕获；长上传不能在结束时借用同一日记的新会话资格。
#[derive(Clone)]
pub(crate) struct DiaryWritePermit {
    manager: DiaryEditManager,
    id: String,
    session: Arc<EditSession>,
}

impl DiaryEditManager {
    pub fn new(client: OssClient) -> Self {
        Self::with_storage(Arc::new(OssLeaseStorage(client)))
    }
    fn with_storage(storage: Arc<dyn LeaseStorage>) -> Self {
        Self {
            storage,
            sessions: Arc::new(DashMap::new()),
        }
    }
    pub fn invalidate_all(&self) {
        for entry in self.sessions.iter() {
            entry.valid.store(false, Ordering::SeqCst);
        }
    }
    pub fn capture(&self, id: &str) -> Option<DiaryWritePermit> {
        self.sessions.get(id).map(|entry| DiaryWritePermit {
            manager: self.clone(),
            id: id.into(),
            session: entry.value().clone(),
        })
    }
    pub fn capture_all(&self) -> std::collections::HashMap<String, DiaryWritePermit> {
        self.sessions
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    DiaryWritePermit {
                        manager: self.clone(),
                        id: entry.key().clone(),
                        session: entry.value().clone(),
                    },
                )
            })
            .collect()
    }
    pub fn require_session(&self, id: &str, token: Option<&str>) -> Result<(), DiaryError> {
        let permit = self
            .capture(id)
            .ok_or_else(|| unavailable("当前日记为只读，请重新申请编辑"))?;
        if token != Some(permit.session.token.as_str())
            || !permit.session.valid.load(Ordering::SeqCst)
        {
            return Err(unavailable("编辑会话已失效，本机草稿已保留"));
        }
        Ok(())
    }
    pub async fn acquire(
        &self,
        id: &str,
        takeover: bool,
        baseline: String,
    ) -> Result<DiaryEditAccess, DiaryError> {
        if ObjectLocations::diary_edit_lease_id(&ObjectLocations::diary_edit_lease(id)) != Some(id)
        {
            return Err(unavailable("无效的日记 ID"));
        }
        let (record, now) = self.storage.read(id).await?;
        if let Some(record) = record {
            let owns_record = record.expires_at > now
                && self.sessions.get(id).is_some_and(|session| {
                    session.valid.load(Ordering::SeqCst) && session.token == record.token
                });
            if record.expires_at > now && !owns_record {
                return Ok(DiaryEditAccess::read_only(
                    "其他编辑会话正在占用此日记，可只读查看",
                    false,
                ));
            }
            if record.expires_at != 0 && !owns_record && !takeover {
                return Ok(DiaryEditAccess::read_only(
                    "上次编辑占用已过期，确认后可接管编辑",
                    true,
                ));
            }
        }
        let mut token = [0u8; 16];
        getrandom::fill(&mut token).map_err(|e| unavailable(e.to_string()))?;
        let record = LeaseRecord {
            version: 1,
            diary_id: id.into(),
            token: hex::encode(token),
            expires_at: now + LEASE_MILLIS,
        };
        self.storage.write(id, &record).await?;
        let (confirmed, confirmed_at) = self.storage.read(id).await?;
        if !confirmed.is_some_and(|r| r.token == record.token && r.expires_at > confirmed_at) {
            return Ok(DiaryEditAccess::read_only(
                "编辑占用确认失败，请只读查看",
                false,
            ));
        }
        let session = Arc::new(EditSession {
            token: record.token.clone(),
            valid: AtomicBool::new(true),
            data: Arc::new(Mutex::new(SessionData {
                baseline,
                confirmed: Instant::now(),
            })),
        });
        if let Some(previous) = self.sessions.insert(id.into(), session) {
            previous.valid.store(false, Ordering::SeqCst);
        }
        Ok(DiaryEditAccess {
            session_id: Some(record.token),
            read_only: false,
            can_take_over: false,
            reason: None,
        })
    }
    pub async fn renew(&self, id: &str, token: &str) -> Result<(), DiaryError> {
        self.require_session(id, Some(token))?;
        let permit = self
            .capture(id)
            .ok_or_else(|| unavailable("没有编辑会话"))?;
        let mut data = permit.check().await?;
        let result = async {
            let (record, now) = self.storage.read(id).await?;
            let mut record = record.ok_or_else(|| unavailable("编辑占用已消失"))?;
            if record.token != token || record.expires_at <= now {
                return Err(unavailable("编辑占用已失效"));
            }
            record.expires_at = now + LEASE_MILLIS;
            self.storage.write(id, &record).await?;
            let (record, now) = self.storage.read(id).await?;
            if !record.is_some_and(|r| r.token == token && r.expires_at > now) {
                return Err(unavailable("编辑续期确认失败"));
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            permit.session.valid.store(false, Ordering::SeqCst);
        } else {
            data.confirmed = Instant::now();
        }
        result
    }
    pub async fn release(&self, id: &str, token: &str) -> Result<(), DiaryError> {
        let Some(permit) = self.capture(id) else {
            return Ok(());
        };
        if permit.session.token != token {
            return Ok(());
        }
        let _data = permit.session.data.lock().await;
        permit.session.valid.store(false, Ordering::SeqCst);
        let (record, _) = self.storage.read(id).await?;
        if let Some(mut record) = record.filter(|r| r.token == token) {
            // 不删除文件：0 表示主动释放，下次可直接申请；异常过期才需要手动接管。
            record.expires_at = 0;
            self.storage.write(id, &record).await?;
        }
        Ok(())
    }
}

impl DiaryWritePermit {
    pub fn check_local(&self) -> Result<(), DiaryError> {
        if !self.session.valid.load(Ordering::SeqCst) {
            return Err(unavailable("编辑会话已失效，本机草稿已保留"));
        }
        Ok(())
    }
    pub async fn check(&self) -> Result<OwnedMutexGuard<SessionData>, DiaryError> {
        let data = self.session.data.clone().lock_owned().await;
        let result = async {
            if !self.session.valid.load(Ordering::SeqCst)
                || data.confirmed.elapsed() >= Duration::from_millis(LEASE_MILLIS as u64)
                || !self
                    .manager
                    .sessions
                    .get(&self.id)
                    .is_some_and(|s| Arc::ptr_eq(s.value(), &self.session))
            {
                return Err(unavailable("编辑会话已失效，本机草稿已保留"));
            }
            let (record, now) = self.manager.storage.read(&self.id).await?;
            if !record.is_some_and(|r| r.token == self.session.token && r.expires_at > now) {
                return Err(unavailable("编辑占用已被释放、接管或过期，本机草稿已保留"));
            }
            if self.manager.storage.manifest_etag(&self.id).await? != data.baseline {
                return Err(unavailable("云端日记已变化，已停止保存并保留本机草稿"));
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            self.session.valid.store(false, Ordering::SeqCst);
            return Err(error);
        }
        Ok(data)
    }
    pub fn invalidate(&self) {
        self.session.valid.store(false, Ordering::SeqCst);
    }
}

/// multipart 上传期间只暂存分片；最终发布前再次验证，失败则中止临时上传。
pub(super) struct GuardedAttachmentUpload {
    pub inner: Box<dyn super::AttachmentUploadSession>,
    pub permit: DiaryWritePermit,
}
#[async_trait]
impl super::AttachmentUploadSession for GuardedAttachmentUpload {
    async fn write_chunk(&mut self, data: Vec<u8>) -> Result<(u32, String), DiaryError> {
        self.permit.check_local()?;
        self.inner.write_chunk(data).await
    }
    async fn finish(self: Box<Self>) -> Result<String, DiaryError> {
        match self.permit.check().await {
            Ok(guard) => drop(guard),
            Err(error) => {
                let _ = self.inner.abort().await;
                return Err(error);
            }
        }
        self.inner.finish().await
    }
    async fn abort(self: Box<Self>) -> Result<(), DiaryError> {
        self.inner.abort().await
    }
}

#[cfg(test)]
#[path = "edit_lease_tests.rs"]
mod tests;

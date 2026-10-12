use super::progress::FileProgress;
use super::storage::{RekeyStorage, Storage};
use super::types::Entry;
use super::Result;
use crate::stream::ByteStream;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// 只保存大小、类型和匿名索引，不保存对象 Key、名称、密码、盐或密钥材料。
#[derive(Clone, Debug, Deserialize, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeReport {
    pub schema_version: u32,
    pub run_id: String,
    pub storage_mode: String,
    pub build_mode: String,
    pub platform: String,
    pub architecture: String,
    #[specta(type = f64)]
    pub started_at_ms: u64,
    #[specta(type = f64)]
    pub updated_at_ms: u64,
    pub outcome: String,
    pub attempts: u32,
    /// 强制退出只能保留最近检查点的统计；不能把遗失的耗时冒充完整历史。
    pub incomplete_history: bool,
    pub encrypted_files: u32,
    pub completed_files: u32,
    #[specta(type = f64)]
    pub encrypted_bytes: u64,
    #[specta(type = f64)]
    pub preparation_ms: u64,
    #[specta(type = f64)]
    pub derivation_ms: u64,
    #[specta(type = f64)]
    pub benchmark_ms: u64,
    #[specta(type = f64)]
    pub active_ms: u64,
    #[specta(type = f64)]
    pub read_bytes: u64,
    /// 已交给写入端的字节，不代表失败请求已成功提交，也不是 HTTP 线上流量。
    #[specta(type = f64)]
    pub write_stream_bytes: u64,
    #[specta(type = f64)]
    pub cloud_copy_bytes: u64,
    /// next().await 的累计时间，含网络/磁盘与任务调度，不含消费者处理间隔。
    #[specta(type = f64)]
    pub read_await_ms: u64,
    #[specta(type = f64)]
    pub crypto_ms: u64,
    #[specta(type = f64)]
    pub hash_ms: u64,
    pub operations: Vec<KeyChangeTiming>,
    pub phases: Vec<KeyChangeTiming>,
    pub slowest_files: Vec<KeyChangeFileTiming>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeTiming {
    pub name: String,
    pub count: u32,
    pub failed: u32,
    #[specta(type = f64)]
    pub total_ms: u64,
    #[specta(type = f64)]
    pub max_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyChangeFileTiming {
    pub index: u32,
    pub kind: String,
    #[specta(type = f64)]
    pub bytes: u64,
    #[specta(type = f64)]
    pub elapsed_ms: u64,
    pub completed: bool,
}

#[derive(Clone)]
pub(super) struct Diagnostics(Arc<Mutex<State>>);

#[derive(Default)]
struct Sample {
    count: u32,
    failed: u32,
    total: Duration,
    max: Duration,
}

struct State {
    report: KeyChangeReport,
    active: Duration,
    started: Option<Instant>,
    read_await: Duration,
    crypto: Duration,
    hash: Duration,
    operations: BTreeMap<String, Sample>,
    phases: BTreeMap<String, Sample>,
}

fn milliseconds(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

impl Diagnostics {
    pub fn new(run_id: String, remote: bool, files: u32, bytes: u64) -> Self {
        Self::restore(KeyChangeReport {
            schema_version: 1,
            run_id,
            storage_mode: if remote { "cloud" } else { "local" }.into(),
            build_mode: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            started_at_ms: now_ms(),
            updated_at_ms: now_ms(),
            outcome: "prepared".into(),
            attempts: 0,
            incomplete_history: false,
            encrypted_files: files,
            completed_files: 0,
            encrypted_bytes: bytes,
            preparation_ms: 0,
            derivation_ms: 0,
            benchmark_ms: 0,
            active_ms: 0,
            read_bytes: 0,
            write_stream_bytes: 0,
            cloud_copy_bytes: 0,
            read_await_ms: 0,
            crypto_ms: 0,
            hash_ms: 0,
            operations: vec![],
            phases: vec![],
            slowest_files: vec![],
        })
    }

    pub fn restore(mut report: KeyChangeReport) -> Self {
        report.incomplete_history |= report.outcome == "running";
        let samples = |values: &[KeyChangeTiming]| {
            values
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        Sample {
                            count: s.count,
                            failed: s.failed,
                            total: Duration::from_millis(s.total_ms),
                            max: Duration::from_millis(s.max_ms),
                        },
                    )
                })
                .collect()
        };
        Self(Arc::new(Mutex::new(State {
            active: Duration::from_millis(report.active_ms),
            started: None,
            read_await: Duration::from_millis(report.read_await_ms),
            crypto: Duration::from_millis(report.crypto_ms),
            hash: Duration::from_millis(report.hash_ms),
            operations: samples(&report.operations),
            phases: samples(&report.phases),
            report,
        })))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn preparation(&self, total: Duration, derivation: Duration, benchmark: Duration) {
        let mut state = self.state();
        state.report.preparation_ms = milliseconds(total);
        state.report.derivation_ms = milliseconds(derivation);
        state.report.benchmark_ms = milliseconds(benchmark);
    }

    pub fn begin(&self) {
        let mut state = self.state();
        state.report.attempts = state.report.attempts.saturating_add(1);
        state.report.outcome = "running".into();
        state.started = Some(Instant::now());
    }

    pub fn finish(&self, outcome: &str) {
        let mut state = self.state();
        if let Some(started) = state.started.take() {
            state.active = state.active.saturating_add(started.elapsed());
        }
        state.report.outcome = outcome.into();
        state.report.updated_at_ms = now_ms();
    }

    pub fn completed(&self, count: usize) {
        self.state().report.completed_files = count.min(u32::MAX as usize) as u32;
    }

    pub fn add_active(&self, elapsed: Duration) {
        let mut state = self.state();
        state.active = state.active.saturating_add(elapsed);
    }

    pub fn crypto(&self, elapsed: Duration) {
        let mut state = self.state();
        state.crypto = state.crypto.saturating_add(elapsed);
    }

    pub fn hash(&self, elapsed: Duration) {
        let mut state = self.state();
        state.hash = state.hash.saturating_add(elapsed);
    }

    pub fn snapshot(&self) -> KeyChangeReport {
        let state = self.state();
        let mut report = state.report.clone();
        if state.started.is_some() {
            report.updated_at_ms = now_ms();
        }
        report.active_ms = milliseconds(
            state
                .active
                .saturating_add(state.started.map(|s| s.elapsed()).unwrap_or_default()),
        );
        report.read_await_ms = milliseconds(state.read_await);
        report.crypto_ms = milliseconds(state.crypto);
        report.hash_ms = milliseconds(state.hash);
        let snapshot = |map: &BTreeMap<String, Sample>| {
            map.iter()
                .map(|(name, s)| KeyChangeTiming {
                    name: name.clone(),
                    count: s.count,
                    failed: s.failed,
                    total_ms: milliseconds(s.total),
                    max_ms: milliseconds(s.max),
                })
                .collect()
        };
        report.operations = snapshot(&state.operations);
        report.phases = snapshot(&state.phases);
        report
    }

    pub fn operation(&self, name: &'static str) -> Timer {
        Timer {
            diagnostics: self.clone(),
            name,
            started: Instant::now(),
            success: false,
            phase: false,
        }
    }

    pub fn phases(&self) -> PhaseTimer {
        PhaseTimer {
            diagnostics: self.clone(),
            current: None,
        }
    }

    pub fn file(&self, index: usize, kind: &'static str, bytes: u64) -> FileTimer {
        FileTimer {
            diagnostics: self.clone(),
            started: Instant::now(),
            index,
            kind,
            bytes,
            completed: false,
        }
    }

    fn read_stream(&self, stream: ByteStream) -> ByteStream {
        let diagnostics = self.clone();
        let timer = self.operation("readBody");
        Box::pin(futures_util::stream::unfold(
            (stream, timer, false),
            move |(mut stream, mut timer, failed)| {
                let diagnostics = diagnostics.clone();
                async move {
                    let started = Instant::now();
                    let next = stream.next().await;
                    let mut state = diagnostics.state();
                    state.read_await = state.read_await.saturating_add(started.elapsed());
                    if let Some(Ok(bytes)) = &next {
                        state.report.read_bytes =
                            state.report.read_bytes.saturating_add(bytes.len() as u64);
                    }
                    drop(state);
                    match next {
                        Some(chunk) => {
                            let failed = failed || chunk.is_err();
                            Some((chunk, (stream, timer, failed)))
                        }
                        None => {
                            timer.success = !failed;
                            drop(timer);
                            None
                        }
                    }
                }
            },
        ))
    }

    fn write_stream(&self, stream: ByteStream) -> ByteStream {
        let diagnostics = self.clone();
        Box::pin(stream.map(move |chunk| {
            if let Ok(bytes) = &chunk {
                let mut state = diagnostics.state();
                state.report.write_stream_bytes = state
                    .report
                    .write_stream_bytes
                    .saturating_add(bytes.len() as u64);
            }
            chunk
        }))
    }
}

pub(super) struct Timer {
    diagnostics: Diagnostics,
    name: &'static str,
    started: Instant,
    success: bool,
    phase: bool,
}

impl Timer {
    fn result<T>(&mut self, result: &Result<T>) {
        self.success = result.is_ok();
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        let mut state = self.diagnostics.state();
        let samples = if self.phase {
            &mut state.phases
        } else {
            &mut state.operations
        };
        let sample = samples.entry(self.name.into()).or_default();
        sample.count = sample.count.saturating_add(1);
        sample.failed = sample.failed.saturating_add(u32::from(!self.success));
        sample.total = sample.total.saturating_add(elapsed);
        sample.max = sample.max.max(elapsed);
    }
}

pub(super) struct PhaseTimer {
    diagnostics: Diagnostics,
    current: Option<Timer>,
}

impl PhaseTimer {
    pub fn set(&mut self, name: &'static str) {
        if let Some(mut previous) = self.current.take() {
            previous.success = true;
        }
        let mut timer = self.diagnostics.operation(name);
        timer.phase = true;
        self.current = Some(timer);
    }

    pub fn complete(&mut self) {
        if let Some(mut timer) = self.current.take() {
            timer.success = true;
        }
    }
}

pub(super) struct FileTimer {
    diagnostics: Diagnostics,
    started: Instant,
    index: usize,
    kind: &'static str,
    bytes: u64,
    pub completed: bool,
}

impl Drop for FileTimer {
    fn drop(&mut self) {
        let entry = KeyChangeFileTiming {
            index: self.index.min(u32::MAX as usize) as u32,
            kind: self.kind.into(),
            bytes: self.bytes,
            elapsed_ms: milliseconds(self.started.elapsed()),
            completed: self.completed,
        };
        let mut state = self.diagnostics.state();
        state.report.slowest_files.push(entry);
        state
            .report
            .slowest_files
            .sort_by_key(|f| std::cmp::Reverse(f.elapsed_ms));
        state.report.slowest_files.truncate(10);
    }
}

pub(super) fn crypto<T>(diagnostics: Option<&Diagnostics>, work: impl FnOnce() -> T) -> T {
    let started = diagnostics.map(|_| Instant::now());
    let result = work();
    if let (Some(diagnostics), Some(started)) = (diagnostics, started) {
        diagnostics.crypto(started.elapsed());
    }
    result
}

pub(super) fn hash<T>(diagnostics: Option<&Diagnostics>, work: impl FnOnce() -> T) -> T {
    let started = diagnostics.map(|_| Instant::now());
    let result = work();
    if let (Some(diagnostics), Some(started)) = (diagnostics, started) {
        diagnostics.hash(started.elapsed());
    }
    result
}

/// 统计同一条数据流，不额外读取对象；操作数是逻辑存储调用数，不冒充 HTTP 请求数。
pub(super) struct ObservedStorage<'a> {
    pub inner: &'a Storage,
    pub diagnostics: Diagnostics,
}

#[async_trait]
impl RekeyStorage for ObservedStorage<'_> {
    fn diagnostics(&self) -> Option<Diagnostics> {
        Some(self.diagnostics.clone())
    }

    async fn list(&self) -> Result<Vec<Entry>> {
        self.list_prefix("").await
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<Entry>> {
        let mut timer = self.diagnostics.operation("list");
        let result = self.inner.list_prefix(prefix).await;
        timer.result(&result);
        result
    }

    async fn read(&self, key: &str) -> Result<ByteStream> {
        let mut timer = self.diagnostics.operation("readOpen");
        let result = self.inner.read(key).await;
        timer.result(&result);
        result.map(|stream| self.diagnostics.read_stream(stream))
    }

    async fn head(&self, key: &str) -> Result<Option<Entry>> {
        let mut timer = self.diagnostics.operation("head");
        let result = self.inner.head(key).await;
        timer.result(&result);
        result
    }

    async fn write(&self, key: &str, size: u64, stream: ByteStream) -> Result<String> {
        let mut timer = self.diagnostics.operation("write");
        let result = self
            .inner
            .write(key, size, self.diagnostics.write_stream(stream))
            .await;
        timer.result(&result);
        result
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut timer = self.diagnostics.operation("delete");
        let result = self.inner.delete(key).await;
        timer.result(&result);
        result
    }

    async fn checkpoint(&self, journal: &str) -> Result<()> {
        let mut timer = self.diagnostics.operation("checkpoint");
        let result = self.inner.checkpoint(journal).await;
        timer.result(&result);
        result
    }

    async fn copy(
        &self,
        source: &str,
        target: &str,
        size: u64,
        progress: &FileProgress,
    ) -> Result<String> {
        let mut timer = self.diagnostics.operation("copy");
        let result = if self.inner.remote {
            self.inner.copy(source, target, size, progress).await
        } else {
            self.write(target, size, progress.wrap(self.read(source).await?))
                .await
        };
        timer.result(&result);
        if self.inner.remote && result.is_ok() {
            let mut state = self.diagnostics.state();
            state.report.cloud_copy_bytes = state.report.cloud_copy_bytes.saturating_add(size);
        }
        result
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;

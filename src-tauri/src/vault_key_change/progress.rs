use super::types::{
    KeyChangeFilePath, KeyChangeFileProgress, KeyChangeFileStep, KeyChangeProgress,
};
use crate::stream::ByteStream;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};

pub(super) type Notify = Arc<dyn Fn(KeyChangeProgress) + Send + Sync>;

const MIN_FILE_BYTES: u64 = 10 * 1024 * 1024;
const LARGE_FILE_BYTES: u64 = 1024 * 1024 * 1024;

fn progress_points(size: u64) -> Option<u32> {
    match size {
        ..MIN_FILE_BYTES => None,
        MIN_FILE_BYTES..LARGE_FILE_BYTES => Some(10),
        _ => Some(100),
    }
}

struct State {
    bytes: u64,
    points: u32,
    finished: bool,
}

struct Inner {
    base: KeyChangeProgress,
    step: KeyChangeFileStep,
    path: KeyChangeFilePath,
    size: u64,
    points: u32,
    state: Mutex<State>,
    notify: Notify,
}

/// 包装既有流而不增加读取、缓存或检查点写入。每个步骤最多发送 10/100 个刻度。
/// 流读完不等于写入/校验成功，只有调用方确认成功后 finish 才发送 100%。
#[derive(Clone)]
pub(super) struct FileProgress(Option<Arc<Inner>>);

impl FileProgress {
    pub fn new(
        base: KeyChangeProgress,
        step: KeyChangeFileStep,
        size: u64,
        notify: Notify,
        path: KeyChangeFilePath,
    ) -> Self {
        let tracker = Self(progress_points(size).map(|points| {
            Arc::new(Inner {
                base,
                step,
                path,
                size,
                points,
                state: Mutex::new(State {
                    bytes: 0,
                    points: 0,
                    finished: false,
                }),
                notify,
            })
        }));
        if let Some(inner) = &tracker.0 {
            inner.emit(0, 0);
        }
        tracker
    }

    pub fn wrap(&self, stream: ByteStream) -> ByteStream {
        if self.0.is_none() {
            return stream;
        }
        let tracker = self.clone();
        Box::pin(stream.inspect(move |chunk| {
            if let Ok(chunk) = chunk {
                tracker.advance(chunk.len() as u64);
            }
        }))
    }

    fn advance(&self, bytes: u64) {
        let Some(inner) = &self.0 else { return };
        let mut state = inner.state.lock().expect("改密进度锁损坏");
        if state.finished {
            return;
        }
        state.bytes = state.bytes.saturating_add(bytes).min(inner.size);
        // 使用 u128 避免极大对象乘法溢出；最后一个刻度保留给实际操作成功。
        let points = ((state.bytes as u128 * inner.points as u128 / inner.size as u128) as u32)
            .min(inner.points - 1);
        if points > state.points {
            state.points = points;
            inner.emit(state.bytes, points);
        }
    }

    pub fn finish(&self) {
        if let Some(inner) = &self.0 {
            let mut state = inner.state.lock().expect("改密进度锁损坏");
            if !state.finished {
                state.finished = true;
                inner.emit(inner.size, inner.points);
            }
        }
    }
}

impl Inner {
    fn emit(&self, bytes: u64, points: u32) {
        let mut event = self.base.clone();
        event.file_progress = Some(KeyChangeFileProgress {
            step: self.step.clone(),
            path: self.path.clone(),
            completed_bytes: bytes,
            total_bytes: self.size,
            completed_points: points,
            total_points: self.points,
        });
        (self.notify)(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(size: u64) -> (FileProgress, Arc<Mutex<Vec<KeyChangeProgress>>>) {
        let events = Arc::new(Mutex::new(vec![]));
        let saved = events.clone();
        let base = KeyChangeProgress {
            phase: "verifying".into(),
            completed_files: 2,
            total_files: 3,
            completed_bytes: 123,
            total_bytes: size.saturating_add(123),
            current_file: Some("test.enc".into()),
            file_progress: None,
        };
        (
            FileProgress::new(
                base,
                KeyChangeFileStep::CheckingStageContent,
                size,
                Arc::new(move |p| saved.lock().unwrap().push(p)),
                KeyChangeFilePath::Full,
            ),
            events,
        )
    }

    #[test]
    fn thresholds_use_ten_and_one_hundred_points() {
        assert_eq!(progress_points(0), None);
        assert_eq!(progress_points(MIN_FILE_BYTES - 1), None);
        assert_eq!(progress_points(MIN_FILE_BYTES), Some(10));
        assert_eq!(progress_points(LARGE_FILE_BYTES - 1), Some(10));
        assert_eq!(progress_points(LARGE_FILE_BYTES), Some(100));
        assert_eq!(progress_points(u64::MAX), Some(100));
    }

    #[test]
    fn runtime_path_is_reported_without_changing_durable_progress() {
        for path in [
            KeyChangeFilePath::Full,
            KeyChangeFilePath::CloudFast,
            KeyChangeFilePath::CloudWithBackupRead,
            KeyChangeFilePath::AlreadyCommitted,
        ] {
            let events = Arc::new(Mutex::new(vec![]));
            let saved = events.clone();
            let base = KeyChangeProgress {
                phase: "committing".into(),
                completed_files: 0,
                total_files: 1,
                completed_bytes: 0,
                total_bytes: MIN_FILE_BYTES,
                current_file: Some("file".into()),
                file_progress: None,
            };
            let progress = FileProgress::new(
                base,
                KeyChangeFileStep::CheckingCommitted,
                MIN_FILE_BYTES,
                Arc::new(move |p| saved.lock().unwrap().push(p)),
                path.clone(),
            );
            progress.advance(MIN_FILE_BYTES / 2);
            progress.finish();
            for event in events.lock().unwrap().iter() {
                assert_eq!(event.file_progress.as_ref().unwrap().path, path);
                assert_eq!(event.completed_bytes, 0);
                assert_eq!(event.completed_files, 0);
            }
        }
    }

    #[tokio::test]
    async fn large_stream_is_bounded_quantized_and_finishes_only_after_acknowledgement() {
        // 重用同一小缓冲模拟 1 GiB 流，不分配 1 GiB 内存或实际上传。
        for size in [MIN_FILE_BYTES, LARGE_FILE_BYTES] {
            let (tracker, events) = tracker(size);
            let chunk = bytes::Bytes::from(vec![0; 1024 * 1024]);
            let stream = futures_util::stream::iter(
                (0..size / chunk.len() as u64).map(move |_| Ok(chunk.clone())),
            );
            let mut stream = tracker.wrap(Box::pin(stream));
            let mut total = 0;
            while let Some(chunk) = stream.next().await {
                total += chunk.unwrap().len() as u64;
            }
            assert_eq!(total, size);
            assert!(events.lock().unwrap().iter().all(|p| {
                let file = p.file_progress.as_ref().unwrap();
                file.completed_points < file.total_points
                    && p.completed_files == 2
                    && p.completed_bytes == 123
            }));
            tracker.finish();
            tracker.finish();
            let events = events.lock().unwrap();
            let points: Vec<_> = events
                .iter()
                .map(|p| p.file_progress.as_ref().unwrap().completed_points)
                .collect();
            assert_eq!(
                points,
                (0..=progress_points(size).unwrap()).collect::<Vec<_>>()
            );
            assert_eq!(
                events
                    .last()
                    .unwrap()
                    .file_progress
                    .as_ref()
                    .unwrap()
                    .completed_bytes,
                size
            );
        }
    }

    #[tokio::test]
    async fn errors_and_small_files_do_not_report_false_completion() {
        let (small, small_events) = tracker(MIN_FILE_BYTES - 1);
        small.advance(u64::MAX);
        small.finish();
        assert!(small_events.lock().unwrap().is_empty());
        let (large, events) = tracker(MIN_FILE_BYTES);
        let stream: ByteStream = Box::pin(futures_util::stream::iter([
            Ok(bytes::Bytes::from(vec![0; MIN_FILE_BYTES as usize])),
            Err(std::io::Error::other("断流")),
        ]));
        let mut stream = large.wrap(stream);
        assert!(stream.next().await.unwrap().is_ok());
        assert!(stream.next().await.unwrap().is_err());
        assert!(events.lock().unwrap().iter().all(|p| {
            let file = p.file_progress.as_ref().unwrap();
            file.completed_points < file.total_points
        }));
    }

    #[test]
    fn oversized_counter_does_not_overflow_or_finish_early() {
        let (tracker, events) = tracker(u64::MAX);
        tracker.advance(u64::MAX);
        tracker.advance(u64::MAX);
        assert_eq!(
            events
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .file_progress
                .as_ref()
                .unwrap()
                .completed_points,
            99
        );
        tracker.finish();
        assert_eq!(
            events
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .file_progress
                .as_ref()
                .unwrap()
                .completed_points,
            100
        );
    }
}

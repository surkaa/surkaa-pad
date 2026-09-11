use crate::attachments::AttachmentMeta;
use crate::cryptos::Crypto;
use crate::diaries::DiaryStore;
use futures_util::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Seek, SeekFrom};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

const RANGE_CHUNK_SIZE: u64 = 128 * 1024;
const MAX_CACHED_CHUNKS: usize = 8;
const MAX_FETCHED_BYTES: u64 = 32 * 1024 * 1024;

pub(super) trait ArchiveRangeSource {
    fn read_range(&mut self, start: u64, end: u64) -> io::Result<Vec<u8>>;
}

pub(super) struct DiaryAttachmentRangeSource {
    runtime: Handle,
    store: Box<dyn DiaryStore>,
    crypto: Crypto,
    diary_id: String,
    attachment: AttachmentMeta,
    cancellation: CancellationToken,
}

impl DiaryAttachmentRangeSource {
    pub(super) fn new(
        runtime: Handle,
        store: Box<dyn DiaryStore>,
        crypto: Crypto,
        diary_id: String,
        attachment: AttachmentMeta,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            runtime,
            store,
            crypto,
            diary_id,
            attachment,
            cancellation,
        }
    }
}

impl ArchiveRangeSource for DiaryAttachmentRangeSource {
    fn read_range(&mut self, start: u64, end: u64) -> io::Result<Vec<u8>> {
        if self.cancellation.is_cancelled() {
            return Err(cancelled_io());
        }
        let expected = end
            .checked_sub(start)
            .and_then(|size| size.checked_add(1))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "无效的附件范围"))?;
        let store = &*self.store;
        let crypto = &self.crypto;
        let diary_id = &self.diary_id;
        let attachment = &self.attachment;
        let cancellation = &self.cancellation;
        self.runtime.block_on(async {
            let stream = tokio::select! {
                _ = cancellation.cancelled() => return Err(cancelled_io()),
                result = store.download_attachment(
                    diary_id,
                    &attachment.id,
                    Some((start, end)),
                    attachment.etag.as_deref(),
                ) => result.map_err(|error| io::Error::other(error.to_string()))?,
            };
            let mut stream = if attachment.encrypted {
                crypto
                    .decrypt_streaming(stream, &attachment.nonce, start)
                    .map_err(|error| io::Error::other(error.to_string()))?
            } else {
                stream
            };
            let mut data = Vec::with_capacity(usize::try_from(expected).unwrap_or(0));
            loop {
                let next = tokio::select! {
                    _ = cancellation.cancelled() => return Err(cancelled_io()),
                    next = stream.next() => next,
                };
                match next {
                    Some(Ok(chunk)) => {
                        if data.len().saturating_add(chunk.len()) > expected as usize {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "附件 Range 响应超过预期长度",
                            ));
                        }
                        data.extend_from_slice(&chunk);
                    }
                    Some(Err(error)) => return Err(error),
                    None => break,
                }
            }
            if data.len() as u64 != expected {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "附件 Range 响应长度错误：预期 {expected}，实际 {}",
                        data.len()
                    ),
                ));
            }
            Ok(data)
        })
    }
}

pub(super) struct SeekableRangeReader<S> {
    source: S,
    length: u64,
    position: u64,
    cache: HashMap<u64, Vec<u8>>,
    lru: VecDeque<u64>,
    fetched_bytes: u64,
}

impl<S> SeekableRangeReader<S> {
    pub(super) fn new(source: S, length: u64) -> Self {
        Self {
            source,
            length,
            position: 0,
            cache: HashMap::new(),
            lru: VecDeque::new(),
            fetched_bytes: 0,
        }
    }
}

impl<S: ArchiveRangeSource> SeekableRangeReader<S> {
    fn chunk(&mut self, start: u64) -> io::Result<&[u8]> {
        if self.cache.contains_key(&start) {
            self.touch(start);
            return Ok(self.cache.get(&start).expect("缓存条目刚刚已确认存在"));
        }
        let end = start
            .saturating_add(RANGE_CHUNK_SIZE.saturating_sub(1))
            .min(self.length.saturating_sub(1));
        let expected = end.saturating_sub(start).saturating_add(1);
        if self.fetched_bytes.saturating_add(expected) > MAX_FETCHED_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "压缩包目录读取量超过 32 MiB 安全限制",
            ));
        }
        let data = self.source.read_range(start, end)?;
        if data.len() as u64 != expected {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "压缩包数据块长度不完整",
            ));
        }
        self.fetched_bytes += expected;
        self.cache.insert(start, data);
        self.touch(start);
        while self.lru.len() > MAX_CACHED_CHUNKS {
            if let Some(evicted) = self.lru.pop_front() {
                self.cache.remove(&evicted);
            }
        }
        Ok(self.cache.get(&start).expect("刚插入的缓存条目不存在"))
    }

    fn touch(&mut self, start: u64) {
        if let Some(index) = self.lru.iter().position(|cached| *cached == start) {
            self.lru.remove(index);
        }
        self.lru.push_back(start);
    }
}

impl<S: ArchiveRangeSource> Read for SeekableRangeReader<S> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position >= self.length {
            return Ok(0);
        }
        let mut written = 0;
        while written < buffer.len() && self.position < self.length {
            let chunk_start = self.position / RANGE_CHUNK_SIZE * RANGE_CHUNK_SIZE;
            let offset = usize::try_from(self.position - chunk_start)
                .map_err(|_| io::Error::other("压缩包偏移量超出平台限制"))?;
            let available = {
                let chunk = self.chunk(chunk_start)?;
                let remaining = chunk.len().saturating_sub(offset);
                remaining
                    .min(buffer.len() - written)
                    .min(usize::try_from(self.length - self.position).unwrap_or(usize::MAX))
            };
            if available == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "压缩包数据块为空",
                ));
            }
            let chunk = self
                .cache
                .get(&chunk_start)
                .expect("已读取的数据块不在缓存中");
            buffer[written..written + available]
                .copy_from_slice(&chunk[offset..offset + available]);
            written += available;
            self.position += available as u64;
        }
        Ok(written)
    }
}

impl<S> Seek for SeekableRangeReader<S> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if !(0..=i128::from(u64::MAX)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "压缩包 Seek 超出有效范围",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

fn cancelled_io() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "压缩包预览已取消")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MemoryRangeSource {
        data: Vec<u8>,
        requests: Arc<Mutex<Vec<(u64, u64)>>>,
    }

    type TestReader = SeekableRangeReader<MemoryRangeSource>;
    type RangeRequests = Arc<Mutex<Vec<(u64, u64)>>>;

    impl ArchiveRangeSource for MemoryRangeSource {
        fn read_range(&mut self, start: u64, end: u64) -> io::Result<Vec<u8>> {
            self.requests.lock().unwrap().push((start, end));
            Ok(self.data[start as usize..=end as usize].to_vec())
        }
    }

    fn reader(data: Vec<u8>) -> (TestReader, RangeRequests) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let source = MemoryRangeSource {
            data,
            requests: requests.clone(),
        };
        let length = source.data.len() as u64;
        (SeekableRangeReader::new(source, length), requests)
    }

    #[test]
    fn reads_and_seeks_across_range_chunks() {
        let data: Vec<u8> = (0..RANGE_CHUNK_SIZE * 2 + 17)
            .map(|index| (index % 251) as u8)
            .collect();
        let (mut reader, requests) = reader(data.clone());
        reader.seek(SeekFrom::Start(RANGE_CHUNK_SIZE - 3)).unwrap();
        let mut output = [0_u8; 9];
        reader.read_exact(&mut output).unwrap();
        assert_eq!(
            output,
            data[(RANGE_CHUNK_SIZE - 3) as usize..(RANGE_CHUNK_SIZE + 6) as usize]
        );
        assert_eq!(requests.lock().unwrap().len(), 2);

        reader.seek(SeekFrom::Start(RANGE_CHUNK_SIZE + 1)).unwrap();
        reader.read_exact(&mut output[..3]).unwrap();
        assert_eq!(
            requests.lock().unwrap().len(),
            2,
            "缓存命中不应重复读取范围"
        );
    }

    #[test]
    fn rejects_seek_before_start_and_reads_eof() {
        let (mut reader, _) = reader(vec![1, 2, 3]);
        assert!(reader.seek(SeekFrom::End(-4)).is_err());
        reader.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(reader.read(&mut [0_u8; 1]).unwrap(), 0);
    }
}

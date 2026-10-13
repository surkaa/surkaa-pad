use super::{error, Result};
use crate::stream::ByteStream;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const OSS_CRC: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);

#[derive(Clone, Default)]
pub(super) struct StreamDigest(Arc<Mutex<State>>, Option<super::diagnostics::Diagnostics>);

struct State {
    sha: Sha256,
    size: u64,
    complete: bool,
    crc: Option<crc::Digest<'static, u64>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sha: Sha256::new(),
            size: 0,
            complete: false,
            crc: None,
        }
    }
}

impl StreamDigest {
    pub fn observed(diagnostics: Option<&super::diagnostics::Diagnostics>) -> Self {
        Self(Default::default(), diagnostics.cloned())
    }
    pub fn with_crc(diagnostics: Option<&super::diagnostics::Diagnostics>) -> Self {
        Self(
            Arc::new(Mutex::new(State {
                crc: Some(OSS_CRC.digest()),
                ..Default::default()
            })),
            diagnostics.cloned(),
        )
    }
    /// 在实际消费的同一条流上计算哈希，不缓存内容或额外读取对象。
    pub fn wrap(&self, stream: ByteStream) -> ByteStream {
        let digest = self.clone();
        Box::pin(futures_util::stream::try_unfold(
            (stream, digest),
            |(mut stream, digest)| async move {
                match stream.next().await {
                    Some(chunk) => {
                        let chunk = chunk?;
                        {
                            let mut state = digest.0.lock().expect("改密哈希锁损坏");
                            state.size = state
                                .size
                                .checked_add(chunk.len() as u64)
                                .ok_or_else(|| std::io::Error::other("对象大小溢出"))?;
                            super::diagnostics::hash(digest.1.as_ref(), || {
                                state.sha.update(&chunk);
                                if let Some(crc) = &mut state.crc {
                                    crc.update(&chunk);
                                }
                            });
                        }
                        Ok(Some((chunk, (stream, digest))))
                    }
                    None => {
                        digest.0.lock().expect("改密哈希锁损坏").complete = true;
                        Ok(None)
                    }
                }
            },
        ))
    }

    pub fn result(&self) -> Result<(u64, String)> {
        let state = self.0.lock().expect("改密哈希锁损坏");
        // 已消费的字节数相等也不代表成功：必须收到 EOF，且调用方须先确认写入成功。
        if !state.complete {
            return Err(error("改密数据流未完整读取，未替换原对象"));
        }
        Ok((state.size, hex::encode(state.sha.clone().finalize())))
    }

    pub fn crc64(&self) -> Result<u64> {
        let state = self.0.lock().expect("改密哈希锁损坏");
        if !state.complete {
            return Err(error("CRC 数据流未完整读取"));
        }
        state
            .crc
            .as_ref()
            .map(|crc| crc.clone().finalize())
            .ok_or_else(|| error("未计算源 CRC"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn oss_crc_is_xz_and_requires_eof_and_handles_arbitrary_chunks() {
        for (bytes, expected) in [
            (b"".as_slice(), 0),
            (b"123456789".as_slice(), 0x995dc9bbdf1939fa),
        ] {
            for chunk_size in 1..=9 {
                let digest = StreamDigest::with_crc(None);
                assert!(digest.crc64().is_err());
                let chunks: Vec<_> = bytes
                    .chunks(chunk_size)
                    .map(|v| Ok(Bytes::copy_from_slice(v)))
                    .collect();
                let mut stream = digest.wrap(Box::pin(futures_util::stream::iter(chunks)));
                while let Some(chunk) = stream.next().await {
                    chunk.unwrap();
                    assert!(digest.crc64().is_err());
                }
                assert_eq!(digest.crc64().unwrap(), expected);
                assert_eq!(
                    digest.result().unwrap().1,
                    hex::encode(Sha256::digest(bytes))
                );
            }
        }
        let digest = StreamDigest::with_crc(None);
        let stream: ByteStream = Box::pin(futures_util::stream::iter([
            Ok(Bytes::from_static(b"123")),
            Err(std::io::Error::other("断流")),
        ]));
        assert!(crate::stream::collect_data(digest.wrap(stream))
            .await
            .is_err());
        assert!(digest.crc64().is_err());
    }

    #[tokio::test]
    async fn hashes_empty_and_uneven_chunks_without_changing_content() {
        for chunks in [vec![], vec![b"a".as_slice(), b"", b"bc", b"defg"]] {
            let expected = chunks.concat();
            let digest = StreamDigest::default();
            let stream: ByteStream = Box::pin(futures_util::stream::iter(
                chunks.into_iter().map(|c| Ok(Bytes::copy_from_slice(c))),
            ));
            let actual = crate::stream::collect_data(digest.wrap(stream))
                .await
                .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                digest.result().unwrap(),
                (
                    expected.len() as u64,
                    hex::encode(Sha256::digest(&expected))
                )
            );
        }
    }

    #[tokio::test]
    async fn requires_eof_even_after_all_declared_bytes_are_read() {
        let digest = StreamDigest::default();
        let mut stream = digest.wrap(super::super::storage::byte_stream(b"abc".to_vec()));
        assert!(digest.result().is_err());
        assert_eq!(stream.next().await.unwrap().unwrap(), b"abc".as_slice());
        assert!(digest.result().is_err());
        assert!(stream.next().await.is_none());
        assert_eq!(digest.result().unwrap().0, 3);
    }

    #[tokio::test]
    async fn failure_and_dropped_partial_stream_cannot_produce_a_valid_digest() {
        let digest = StreamDigest::default();
        let stream: ByteStream = Box::pin(futures_util::stream::iter([
            Ok(Bytes::from_static(b"abc")),
            Err(std::io::Error::other("断流")),
        ]));
        let mut stream = digest.wrap(stream);
        assert!(stream.next().await.unwrap().is_ok());
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
        assert!(digest.result().is_err());

        let digest = StreamDigest::default();
        let mut stream = digest.wrap(super::super::storage::byte_stream(b"abc".to_vec()));
        stream.next().await.unwrap().unwrap();
        drop(stream);
        assert!(digest.result().is_err());
    }

    #[tokio::test]
    async fn rejects_size_overflow() {
        let digest = StreamDigest::default();
        digest.0.lock().unwrap().size = u64::MAX;
        let mut stream = digest.wrap(super::super::storage::byte_stream(vec![1]));
        assert!(stream.next().await.unwrap().is_err());
        assert!(digest.result().is_err());
    }
}

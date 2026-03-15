use super::ByteStream;
use bytes::Bytes;
use futures_util::stream;

pub fn create_mock_stream(data: Vec<u8>, chunk_size: usize) -> ByteStream {
    let chunks: Vec<_> = data
        .chunks(chunk_size)
        .map(|chunk| Ok(Bytes::from(chunk.to_vec())))
        .collect();

    Box::pin(stream::iter(chunks))
}

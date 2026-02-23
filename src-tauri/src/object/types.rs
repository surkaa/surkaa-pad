use std::io::Error;
use std::pin::Pin;
use bytes::Bytes;
use futures::Stream;

pub type NextToken = Option<String>;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;

#[cfg(test)]
pub fn create_mock_stream(data: Vec<u8>, chunk_size: usize) -> ByteStream {
    use futures_util::stream;
    let chunks: Vec<_> = data
        .chunks(chunk_size)
        .map(|chunk| Ok(Bytes::from(chunk.to_vec())))
        .collect();

    Box::pin(stream::iter(chunks))
}
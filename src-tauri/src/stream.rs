use bytes::Bytes;
use futures::Stream;
use std::io::Error;
use std::pin::Pin;

pub mod tracker_stream;
pub mod mock_stream;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;

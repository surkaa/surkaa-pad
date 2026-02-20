use std::io::Error;
use std::pin::Pin;
use bytes::Bytes;
use futures::Stream;

pub type NextToken = Option<String>;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;

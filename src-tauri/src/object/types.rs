use bytes::Bytes;
use futures::Stream;
use std::io::Error;
use std::pin::Pin;

pub type NextToken = Option<String>;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;


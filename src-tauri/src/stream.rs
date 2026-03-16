use bytes::Bytes;
use futures::Stream;
use std::io::Error;
use std::pin::Pin;

mod collect_data;
mod mock_stream;
mod tracker_stream;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;
pub use collect_data::{collect_data, collect_data_with_capacity};
pub use mock_stream::create_mock_stream;
pub use tracker_stream::tracker_stream;

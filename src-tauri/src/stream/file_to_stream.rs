use crate::stream::ByteStream;
use std::fs::File;
use tokio_util::io::ReaderStream;

pub fn file_to_stream(file: File) -> ByteStream {
    let tokio_file = tokio::fs::File::from_std(file);
    Box::pin(ReaderStream::new(tokio_file))
}

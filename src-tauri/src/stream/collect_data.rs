use crate::stream::ByteStream;
use futures_util::StreamExt;

pub async fn collect_data(mut stream: ByteStream) -> Result<Vec<u8>, std::io::Error> {
    let mut decrypted = Vec::new();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        decrypted.extend_from_slice(&chunk);
    }
    Ok(decrypted)
}

pub async fn collect_data_with_capacity(
    mut stream: ByteStream,
    size: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let mut decrypted = Vec::with_capacity(size);
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        decrypted.extend_from_slice(&chunk);
    }
    Ok(decrypted)
}

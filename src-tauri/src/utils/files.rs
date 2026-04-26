use std::fs::File;
use std::io::{self, Read, Seek};

#[derive(Debug, thiserror::Error)]
pub enum UtilsError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("File is empty")]
    EmptyFile,
}

pub fn file_size(file: &File) -> Result<u64, io::Error> {
    let metadata = file.metadata()?;
    Ok(metadata.len())
}

pub fn file_mimetype(mut file: File) -> Result<(String, File), UtilsError> {
    let mut buffer = [0; 128];
    let n = file.read(&mut buffer)?;
    if n == 0 {
        return Err(UtilsError::EmptyFile);
    }
    let mimetype = infer::get(&buffer[..n])
        .map(|t| t.mime_type().to_string())
        .unwrap_or_default();

    file.seek(io::SeekFrom::Start(0))?;

    Ok((mimetype, file))
}

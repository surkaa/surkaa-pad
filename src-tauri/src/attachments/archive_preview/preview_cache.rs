//! 预览内容以独立临时密钥分块认证加密落盘；不属于 LOS，不参与云同步或改密。
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use tempfile::TempDir;
use zeroize::{Zeroize, Zeroizing};

const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;

pub(crate) struct EncryptedPreviewFile {
    _lock: File,
    directory: TempDir,
    key: Zeroizing<[u8; 32]>,
    nonce_prefix: [u8; 8],
    pub length: u64,
}

pub(crate) struct PreviewWriter {
    output: File,
    resource: EncryptedPreviewFile,
    buffer: Zeroizing<Vec<u8>>,
    index: u32,
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn nonce(prefix: [u8; 8], index: u32) -> [u8; 12] {
    let mut result = [0; 12];
    result[..8].copy_from_slice(&prefix);
    result[8..].copy_from_slice(&index.to_be_bytes());
    result
}

impl PreviewWriter {
    pub fn new(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let directory = tempfile::Builder::new().prefix("entry-").tempdir_in(root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.path().join("lease"))?;
        fs4::FileExt::try_lock(&lock).map_err(io_error)?;
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.path().join("data.enc"))?;
        let mut key = Zeroizing::new([0; 32]);
        getrandom::fill(key.as_mut()).map_err(io_error)?;
        let mut nonce_prefix = [0; 8];
        getrandom::fill(&mut nonce_prefix).map_err(io_error)?;
        Ok(Self {
            resource: EncryptedPreviewFile {
                _lock: lock,
                directory,
                key,
                nonce_prefix,
                length: 0,
            },
            output,
            buffer: Zeroizing::new(Vec::with_capacity(CHUNK)),
            index: 0,
        })
    }
    fn flush_chunk(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let cipher = Aes256Gcm::new_from_slice(self.resource.key.as_ref()).map_err(io_error)?;
        let nonce = nonce(self.resource.nonce_prefix, self.index);
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce), self.buffer.as_slice())
            .map_err(io_error)?;
        self.output.write_all(&encrypted)?;
        self.resource.length += self.buffer.len() as u64;
        self.index = self
            .index
            .checked_add(1)
            .ok_or_else(|| io_error("临时文件过大"))?;
        self.buffer.zeroize();
        self.buffer.clear();
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<EncryptedPreviewFile> {
        self.flush_chunk()?;
        self.output.flush()?;
        self.output.sync_all()?;
        drop(self.output);
        Ok(self.resource)
    }
}
impl Write for PreviewWriter {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        let total = bytes.len();
        while !bytes.is_empty() {
            let count = (CHUNK - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.buffer.len() == CHUNK {
                self.flush_chunk()?;
            }
        }
        Ok(total)
    }
    fn flush(&mut self) -> io::Result<()> {
        // 不在普通 flush 时固化短块；否则后续 Range 无法按固定块宽度定位。
        self.output.flush()
    }
}

impl EncryptedPreviewFile {
    pub fn read_range(&self, start: u64, length: usize) -> io::Result<Vec<u8>> {
        if start > self.length || length as u64 > self.length - start {
            return Err(io_error("预览范围超出文件长度"));
        }
        let mut result = Vec::with_capacity(length);
        if length == 0 {
            return Ok(result);
        }
        let mut file = File::open(self.directory.path().join("data.enc"))?;
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(io_error)?;
        let mut offset = start;
        while result.len() < length {
            let index = offset / CHUNK as u64;
            let plain_size = (self.length - index * CHUNK as u64).min(CHUNK as u64) as usize;
            file.seek(SeekFrom::Start(index * (CHUNK + TAG) as u64))?;
            let mut encrypted = vec![0; plain_size + TAG];
            file.read_exact(&mut encrypted)?;
            let nonce = nonce(self.nonce_prefix, u32::try_from(index).map_err(io_error)?);
            let plain = Zeroizing::new(
                cipher
                    .decrypt(Nonce::from_slice(&nonce), encrypted.as_slice())
                    .map_err(io_error)?,
            );
            let begin = (offset % CHUNK as u64) as usize;
            let count = (plain_size - begin).min(length - result.len());
            result.extend_from_slice(&plain[begin..begin + count]);
            offset += count as u64;
        }
        Ok(result)
    }
}

/// 清理异常退出留下的密文临时目录；文件锁避免误删另一个仍运行的实例。
pub(crate) fn cleanup_stale(root: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("entry-")
            || !entry.file_type()?.is_dir()
        {
            continue;
        }
        let path = entry.path();
        let Ok(lock) = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.join("lease"))
        else {
            continue;
        };
        if fs4::FileExt::try_lock(&lock).is_err() {
            continue;
        }
        drop(lock);
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_ranges_cross_chunks_and_do_not_leave_plaintext_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..CHUNK * 2 + 11)
            .map(|index| (index % 251) as u8)
            .collect();
        let mut writer = PreviewWriter::new(root.path()).unwrap();
        writer.write_all(&data[..100]).unwrap();
        writer.flush().unwrap();
        writer.write_all(&data[100..]).unwrap();
        let file = writer.finish().unwrap();
        let path = file.directory.path().to_path_buf();
        assert_eq!(file.read_range(0, data.len()).unwrap(), data);
        assert_eq!(
            file.read_range((CHUNK - 3) as u64, 19).unwrap(),
            data[CHUNK - 3..CHUNK + 16]
        );
        assert!(file.read_range(file.length, 1).is_err());
        assert!(file.read_range(u64::MAX, 1).is_err());
        assert_ne!(fs::read(path.join("data.enc")).unwrap()[..64], data[..64]);
        cleanup_stale(root.path()).unwrap();
        assert!(path.exists());
        drop(file);
        assert!(!path.exists());
    }
    #[test]
    fn detects_corrupt_cached_ciphertext() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = PreviewWriter::new(root.path()).unwrap();
        writer.write_all(b"private content").unwrap();
        let file = writer.finish().unwrap();
        let mut corrupt = OpenOptions::new()
            .write(true)
            .open(file.directory.path().join("data.enc"))
            .unwrap();
        corrupt.write_all(&[0; 5]).unwrap();
        drop(corrupt);
        assert!(file.read_range(0, file.length as usize).is_err());
    }
}

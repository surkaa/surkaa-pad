//! 有界单条目解压。只向调用方提供数据流，不使用压缩包路径创建文件。
use super::{detect_format, ensure_entry_count, ArchiveFormat, ArchivePreviewError, EntryLimits};
use serde::Serialize;
use specta::Type;
use std::io::{self, Read, Seek, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub(crate) const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_DICTIONARY_BYTES: u64 = 128 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntryProgress {
    pub phase: ArchiveEntryPhase,
    #[specta(type = f64)]
    pub processed_bytes: u64,
    #[specta(type = f64)]
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveEntryPhase {
    Preparing,
    Extracting,
    Verifying,
}

pub(crate) type ProgressCallback = Arc<dyn Fn(ArchiveEntryProgress) + Send + Sync>;

pub(crate) struct EntryControl {
    cancellation: CancellationToken,
    started: Instant,
    processed: u64,
    total: u64,
    last_percent: u64,
    progress: ProgressCallback,
}

impl EntryControl {
    pub fn new(cancellation: CancellationToken, progress: ProgressCallback) -> Self {
        Self {
            cancellation,
            started: Instant::now(),
            processed: 0,
            total: 0,
            last_percent: u64::MAX,
            progress,
        }
    }
    pub fn check(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "已取消预览"));
        }
        if self.started.elapsed() > Duration::from_secs(300) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "解压超时，请下载后查看",
            ));
        }
        Ok(())
    }
    fn begin(&mut self, total: u64) -> io::Result<()> {
        self.check()?;
        if total > MAX_DECODE_BYTES {
            return Err(io::Error::other("需要解码的压缩块超过 1 GiB，请下载后查看"));
        }
        self.total = total;
        (self.progress)(ArchiveEntryProgress {
            phase: ArchiveEntryPhase::Extracting,
            processed_bytes: 0,
            total_bytes: total,
        });
        Ok(())
    }
    fn copy(
        &mut self,
        reader: &mut dyn Read,
        output: &mut dyn Write,
        expected: u64,
    ) -> io::Result<()> {
        let mut buffer = zeroize::Zeroizing::new(vec![0; CHUNK]);
        let mut copied = 0u64;
        loop {
            self.check()?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .ok_or_else(|| io::Error::other("解压长度溢出"))?;
            self.processed = self.processed.saturating_add(count as u64);
            if copied > expected || self.processed > self.total {
                return Err(io::Error::other("条目实际大小超出声明长度"));
            }
            output.write_all(&buffer[..count])?;
            let percent = self.processed.saturating_mul(100) / self.total.max(1);
            if percent != self.last_percent {
                self.last_percent = percent;
                (self.progress)(ArchiveEntryProgress {
                    phase: ArchiveEntryPhase::Extracting,
                    processed_bytes: self.processed,
                    total_bytes: self.total,
                });
            }
        }
        if copied != expected {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "条目实际大小与目录不一致",
            ));
        }
        Ok(())
    }
}

pub(crate) struct EntryRequest<'a> {
    pub index: usize,
    pub path: &'a str,
    pub size: u64,
    pub password: Option<&'a str>,
}

fn validate_entry(
    path: &str,
    size: u64,
    request: &EntryRequest<'_>,
) -> Result<(), ArchivePreviewError> {
    let path = EntryLimits::default().validate_path(path)?;
    if path != request.path || size != request.size {
        return Err(ArchivePreviewError::InvalidArchive(
            "压缩包条目已变化，请重新打开目录".into(),
        ));
    }
    if size > MAX_ENTRY_BYTES {
        return Err(io::Error::other("单个文件超过 256 MiB 预览上限，请下载后查看").into());
    }
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if ["zip", "7z", "rar", "tar", "gz", "bz2", "xz", "tgz", "zst"].contains(&extension.as_str()) {
        return Err(io::Error::other("暂不支持预览嵌套压缩包").into());
    }
    Ok(())
}

pub(crate) fn extract_entry<R: Read + Seek>(
    mut reader: R,
    request: &EntryRequest<'_>,
    output: &mut dyn Write,
    control: &mut EntryControl,
) -> Result<(), ArchivePreviewError> {
    control.check()?;
    match detect_format(&mut reader)? {
        ArchiveFormat::Zip => {
            let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
            ensure_entry_count(archive.len())?;
            {
                let file = archive.by_index_raw(request.index).map_err(zip_error)?;
                if file.is_dir() || file.is_symlink() {
                    return Err(io::Error::other("不预览目录或符号链接").into());
                }
                validate_entry(file.name(), file.size(), request)?;
                if file.encrypted() && request.password.is_none() {
                    return Err(ArchivePreviewError::PasswordRequired);
                }
            }
            control.begin(request.size)?;
            let mut file = if let Some(password) = request.password {
                archive
                    .by_index_decrypt(request.index, password.as_bytes())
                    .map_err(zip_error)?
            } else {
                archive.by_index(request.index).map_err(zip_error)?
            };
            // 读取到 EOF，让库执行 ZIP CRC / AES 认证；出错的输出不会发布。
            control.copy(&mut file, output, request.size)?;
        }
        ArchiveFormat::SevenZip => {
            let password = sevenz_rust2::Password::new(request.password.unwrap_or_default());
            let archive = read_bounded_seven_zip(&mut reader, &password)?;
            ensure_entry_count(archive.files.len())?;
            let file = archive
                .files
                .get(request.index)
                .ok_or_else(|| io::Error::other("条目索引超出范围"))?;
            if file.is_directory
                || file.is_anti_item
                || (file.has_windows_attributes
                    && (file.windows_attributes & 0x400 != 0
                        || file.windows_attributes >> 16 & 0xf000 == 0xa000))
            {
                return Err(io::Error::other("不预览目录、链接或删除标记").into());
            }
            validate_entry(file.name(), file.size, request)?;
            if !file.has_stream {
                control.begin(0)?;
                if file.size != 0 {
                    return Err(io::Error::other("条目缺少数据流").into());
                }
                return Ok(());
            }
            let block_index = archive.stream_map.file_block_index[request.index]
                .ok_or_else(|| io::Error::other("条目缺少压缩块"))?;
            let block = &archive.blocks[block_index];
            validate_coders(block)?;
            // 完整处理目标固实块，既验证块 CRC，也避免把“目标文件大小”伪装成整体进度。
            let total = block.get_unpack_size();
            control.begin(total)?;
            let (archive, target_index) = stream_files(archive, request.index)?;
            let first = archive.stream_map.block_first_file_index[block_index];
            let mut index = first;
            sevenz_rust2::BlockDecoder::new(1, block_index, &archive, &password, &mut reader)
                .for_each_entries(&mut |entry, data| {
                    let sink: &mut dyn Write = if index == target_index {
                        &mut *output
                    } else {
                        &mut io::sink()
                    };
                    control
                        .copy(data, sink, entry.size)
                        .map_err(|error| sevenz_rust2::Error::Io(error, "解压条目".into()))?;
                    index += 1;
                    Ok(true)
                })
                .map_err(seven_error)?;
            control.check()?;
            if control.processed != total || index <= target_index {
                return Err(io::Error::other("压缩块数据未完整解码").into());
            }
        }
    }
    output.flush()?;
    Ok(())
}

fn stream_files(
    mut archive: sevenz_rust2::Archive,
    target: usize,
) -> Result<(sevenz_rust2::Archive, usize), ArchivePreviewError> {
    let target_index = archive.files[..target]
        .iter()
        .filter(|entry| entry.has_stream)
        .count();
    // BlockDecoder 按子流个数连续遍历 files；目录/空文件穿插在固实块中时不占子流，
    // 为解码器建立仅含流文件的视图，不能把原始目录下标直接当作子流下标。
    let mut counts = vec![0usize; archive.blocks.len()];
    for (file, block) in archive
        .files
        .iter()
        .zip(&archive.stream_map.file_block_index)
    {
        if file.has_stream {
            let count = block
                .and_then(|index| counts.get_mut(index))
                .ok_or_else(|| io::Error::other("文件缺少压缩块映射"))?;
            *count += 1;
        }
    }
    archive.files.retain(|entry| entry.has_stream);
    let mut first = 0usize;
    for (index, count) in counts.into_iter().enumerate() {
        archive.stream_map.block_first_file_index[index] = first;
        first = first
            .checked_add(count)
            .ok_or_else(|| io::Error::other("子流数量溢出"))?;
    }
    if first != archive.files.len() {
        return Err(io::Error::other("压缩块子流与文件数量不一致").into());
    }
    Ok((archive, target_index))
}

fn validate_coders(block: &sevenz_rust2::Block) -> Result<(), ArchivePreviewError> {
    use sevenz_rust2::EncoderMethod;
    for coder in &block.coders {
        let method = coder.encoder_method_id();
        let props = coder.properties();
        let dictionary = if method == EncoderMethod::ID_LZMA {
            if props.len() < 5 {
                return Err(io::Error::other("无效的 LZMA 参数").into());
            }
            u32::from_le_bytes(props[1..5].try_into().unwrap()) as u64
        } else if method == EncoderMethod::ID_LZMA2 {
            let property = props.first().copied().unwrap_or(255);
            if property > 40 {
                return Err(io::Error::other("无效的 LZMA2 参数").into());
            }
            if property == 40 {
                u32::MAX as u64
            } else {
                (2 | (property as u64 & 1)) << (property / 2 + 11)
            }
        } else {
            0
        };
        if dictionary > MAX_DICTIONARY_BYTES {
            return Err(io::Error::other("压缩字典超过 128 MiB 安全上限").into());
        }
        if method == EncoderMethod::ID_AES256_SHA256
            && props.first().is_some_and(|p| p & 63 > 24 && p & 63 != 63)
        {
            return Err(io::Error::other("压缩包密码计算成本过高，无法安全预览").into());
        }
    }
    Ok(())
}

/// 上游读取编码目录时没有暴露内存上限。先借助同一个解析器仅解析 StreamsInfo，
/// 检查目录解码规模和字典，再允许其真正解码；不复制一份 7z 解析/解压实现。
pub(super) fn read_bounded_seven_zip<R: Read + Seek>(
    reader: &mut R,
    password: &sevenz_rust2::Password,
) -> Result<sevenz_rust2::Archive, ArchivePreviewError> {
    use std::io::{Cursor, SeekFrom};
    const MAX_HEADER: u64 = 8 * 1024 * 1024;
    let length = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut signature = [0; 32];
    reader.read_exact(&mut signature)?;
    let offset = u64::from_le_bytes(signature[12..20].try_into().unwrap());
    let size = u64::from_le_bytes(signature[20..28].try_into().unwrap());
    if size == 0
        || size > MAX_HEADER
        || offset
            .checked_add(32)
            .and_then(|pos| pos.checked_add(size))
            .is_none_or(|end| end > length)
    {
        return Err(io::Error::other("7z 目录过大或位置无效，无法安全预览").into());
    }
    reader.seek(SeekFrom::Start(32 + offset))?;
    let mut header = vec![0; size as usize];
    reader.read_exact(&mut header)?;
    if header[0] == 0x17 {
        // EncodedHeader 的 StreamsInfo 与 MainStreamsInfo 编码相同；包装为未压缩头，
        // 得到 coder / unpack_size 元数据，不会执行任何解码或密码派生。
        let mut plain = vec![0x01, 0x04];
        plain.extend_from_slice(&header[1..]);
        plain.push(0);
        let mut synthetic = vec![0; 32];
        synthetic[..8].copy_from_slice(&signature[..8]);
        synthetic[20..28].copy_from_slice(&(plain.len() as u64).to_le_bytes());
        synthetic[28..32].copy_from_slice(&crc32fast::hash(&plain).to_le_bytes());
        let crc = crc32fast::hash(&synthetic[12..32]);
        synthetic[8..12].copy_from_slice(&crc.to_le_bytes());
        synthetic.extend_from_slice(&plain);
        let metadata = sevenz_rust2::Archive::read(
            &mut Cursor::new(synthetic),
            &sevenz_rust2::Password::empty(),
        )
        .map_err(seven_error)?;
        if metadata.blocks.len() != 1 || metadata.blocks[0].get_unpack_size() > MAX_HEADER {
            return Err(io::Error::other("7z 解码目录超过 8 MiB 安全上限").into());
        }
        validate_coders(&metadata.blocks[0])?;
    }
    reader.seek(SeekFrom::Start(0))?;
    sevenz_rust2::Archive::read(reader, password).map_err(seven_error)
}

fn zip_error(error: zip::result::ZipError) -> ArchivePreviewError {
    match error {
        zip::result::ZipError::InvalidPassword => ArchivePreviewError::InvalidPassword,
        zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED) => {
            ArchivePreviewError::PasswordRequired
        }
        other => ArchivePreviewError::InvalidArchive(other.to_string()),
    }
}
fn seven_error(error: sevenz_rust2::Error) -> ArchivePreviewError {
    match error {
        sevenz_rust2::Error::PasswordRequired => ArchivePreviewError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArchivePreviewError::InvalidPassword,
        other => ArchivePreviewError::InvalidArchive(other.to_string()),
    }
}

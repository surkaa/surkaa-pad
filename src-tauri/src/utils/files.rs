use std::fs::File;
use std::io;
use std::io::{Read, Seek};

pub fn file_size(file: &File) -> Result<u64, String> {
    let metadata = file
        .metadata()
        .map_err(|e| format!("无法获取文件元数据: {}", e))?;
    Ok(metadata.len())
}

pub fn file_mimetype(mut file: File) -> Result<(String, File), String> {
    let mut buffer = [0; 128];
    let n = file
        .read(&mut buffer)
        .map_err(|e| format!("无法读取文件内容: {}", e))?;
    if n == 0 {
        return Err("文件为空".to_string());
    }
    let mimetype = infer::get(&buffer[..n])
        .map(|t| t.mime_type().to_string())
        .ok_or_else(|| "无法判断文件类型".to_string())?;

    // 重置文件指针到开头
    file.seek(io::SeekFrom::Start(0))
        .map_err(|e| format!("无法重置文件指针: {}", e))?;

    Ok((mimetype, file))
}

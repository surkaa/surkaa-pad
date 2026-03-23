use crate::stream::ByteStream;
use std::fs::File;
use std::io;
use std::io::{Read, Seek};
use tokio_util::io::ReaderStream;

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

pub fn file_to_stream(file: File) -> ByteStream {
    let tokio_file = tokio::fs::File::from_std(file);
    Box::pin(ReaderStream::new(tokio_file))
}

/// 在 Windows 上直接打开路径
#[cfg(target_os = "windows")]
pub fn open_access_str_file(path: &str) -> io::Result<File> {
    let path = std::path::PathBuf::from(path);
    File::open(path)
}

/// 在 Android 上通过 ContentResolver 获取文件描述符
#[cfg(target_os = "android")]
pub fn open_access_str_file(uri_string: &str) -> io::Result<File> {
    use std::os::unix::io::FromRawFd;

    // 获取 Android Context
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    // Uri uri = Uri.parse(uriString);
    let uri_class = env
        .find_class("android/net/Uri")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let j_uri_string = env
        .new_string(uri_string)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let uri = env
        .call_static_method(
            uri_class,
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[jni::objects::JValue::Object(&j_uri_string)],
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .l()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // ContentResolver resolver = context.getContentResolver();
    let resolver = env
        .call_method(
            context,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .l()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // ParcelFileDescriptor pfd = resolver.openFileDescriptor(uri, "r");
    let mode = env
        .new_string("r")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let pfd_result = env.call_method(
        resolver,
        "openFileDescriptor",
        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
        &[
            jni::objects::JValue::Object(&uri),
            jni::objects::JValue::Object(&mode),
        ],
    );

    if pfd_result.is_err() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Cannot open URI: {}", uri_string),
        ));
    }
    let pfd = pfd_result
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .l()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    if pfd.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "File descriptor is null",
        ));
    }

    // int fd = pfd.detachFd();
    let fd = env
        .call_method(pfd, "detachFd", "()I", &[])
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .i()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // 包装成 Rust File
    let file = unsafe { File::from_raw_fd(fd) };

    Ok(file)
}

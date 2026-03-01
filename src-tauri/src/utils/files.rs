use crate::object::ByteStream;
use std::fs::File;
use std::io;
use std::io::{Read, Seek};
use tokio_util::io::ReaderStream;

pub fn open_file_stream(access_str: String) -> Result<(u64, String, ByteStream), String> {
    let mut file = open_file(&access_str).map_err(|e| format!("无法打开文件{}:{}", access_str, e))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("无法获取文件元数据: {}", e))?;
    let file_size = metadata.len();
    let mut buffer = [0; 128];
    let n = file.read(&mut buffer).map_err(|e| format!("无法读取文件内容: {}", e))?;
    if n == 0 {
        return Err("文件为空".to_string());
    }
    let mimetype = infer::get(&buffer[..n])
        .map(|t| t.mime_type().to_string())
        .ok_or_else(|| "无法判断文件类型".to_string())?;

    // 重置文件指针到开头
    file.seek(io::SeekFrom::Start(0)).map_err(|e| format!("无法重置文件指针: {}", e))?;

    let tokio_file = tokio::fs::File::from_std(file);
    let stream = ReaderStream::new(tokio_file);
    Ok((file_size, mimetype, Box::pin(stream)))
}

/// 在 Windows 上直接打开路径
#[cfg(target_os = "windows")]
fn open_file(path: &str) -> io::Result<File> {
    let path = std::path::PathBuf::from(path);
    File::open(path)
}

/// 在 Android 上通过 ContentResolver 获取文件描述符
#[cfg(target_os = "android")]
fn open_file(uri_string: &str) -> io::Result<File> {
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

/// 在 IOS 未实现
#[cfg(target_os = "ios")]
fn open_file(path: &str) -> io::Result<()> {
    todo!()
}

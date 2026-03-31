use crate::attachments::attachment_types::AttachmentProcessEvent;
use crate::attachments::{get_full_attachment_url, AttachmentMeta};
use crate::caches::{DiaryMemoryCache, LocalFileCache};
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use crate::cryptos::Crypto;
use crate::diaries::{delete_diary_attachment, get_diary, update_diary_attachment};
use crate::object::OssClient;
use crate::storages::remote_attachments_key;
use crate::stream::ByteStream;
use crate::stream::{collect_data, create_mock_stream, tracker_stream};
use crate::utils::message_sender::MessageSender;
use dashmap::DashMap;
use futures_util::StreamExt;
use image::ImageFormat;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Write};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

// 添加附件的锁
static DIARY_ALLOCATORS: LazyLock<DashMap<String, Arc<Mutex<HashSet<u32>>>>> =
    LazyLock::new(DashMap::new);

// 删除附件的互斥锁
static DELETE_LOCKS: LazyLock<DashMap<String, Arc<Mutex<()>>>> = LazyLock::new(DashMap::new);

pub async fn get_attachment_stream(
    key: &str,
    lfc: &LocalFileCache,
    client: &OssClient,
    range: Option<(u64, u64)>,
) -> Result<(ByteStream, u64), String> {
    if let Some(etag) = lfc.get(key).await? {
        // 本地有缓存就判断是否是最新的
        let metadata = client
            .get_metadata(key)
            .await
            .map_err(|e| format!("未能获取附件元数据: {}", e))?;
        if metadata.etag() == etag {
            Ok((lfc.get_stream(key, range).await?, metadata.size()))
        } else {
            // 删除本地过期的缓存
            lfc.delete(key).await;
            Ok(client.download(key, range).await?)
        }
    } else {
        Ok(client.download(key, range).await?)
    }
}

pub async fn add_attachment(
    (crypto, cache, lfc, client): (Crypto, DiaryMemoryCache, LocalFileCache, OssClient),
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    encrypted: bool,
    size: u64,
    mimetype: String,
    stream: ByteStream,
) {
    let _ = event.send(AttachmentProcessEvent::Started);
    // 包装流 用来更新进度
    let ec = event.clone();
    let stream = tracker_stream(size, stream, move |progress| {
        let _ = ec.send(AttachmentProcessEvent::Progress(progress));
    });

    let logic = async move {
        let alloc_state = DIARY_ALLOCATORS.entry(id.to_string()).or_default().clone();
        // 加锁获取模拟状态并执行 MEX 算法
        let allocated_id = {
            let mut pending_ids = alloc_state.lock().await;
            let diary = get_diary(&cache, &lfc, &crypto, &client, id).await?;

            // 提取已落盘的附件序号
            let existing_ids: HashSet<u32> = diary
                .attachments
                .iter()
                .filter_map(|att| att.filename.parse::<u32>().ok())
                .collect();

            // MEX 算法：寻找最小且不重复的正整数序号
            let new_id = (1..).find(|i| !existing_ids.contains(i) && !pending_ids.contains(i));
            let new_id = match new_id {
                Some(id) => id,
                None => return Err("无法分配新的附件 ID".to_string()),
            };

            // 登记到模拟状态中，防止其他并发任务抢占
            pending_ids.insert(new_id);
            new_id
        };
        let filename = allocated_id.to_string();

        // 直接上传
        let key = remote_attachments_key(id, &filename);
        let upload_task = async {
            // 根据是否加密决定最终的流
            let (final_stream, nonce) = if !encrypted {
                (stream, vec![])
            } else {
                crypto.encrypt_streaming(stream)?
            };

            // 包装流用于本地文件缓存
            let (wrapped_stream, handle) = lfc.save(&key, final_stream).await?;

            // 执行上传
            match client.upload(&key, size, wrapped_stream, &mimetype).await {
                Ok(etag) => {
                    // 上传成功，固化本地缓存
                    let _ = handle.finalize(&etag).await;
                    Ok(AttachmentMeta {
                        filename: filename.clone(),
                        mimetype,
                        size,
                        nonce,
                        encrypted,
                        algorithm: Ctr,
                    })
                }
                Err(e) => {
                    // 上传失败，丢弃本地缓存临时文件
                    handle.abort().await;
                    Err(e)
                }
            }
        };

        let upload_result: Result<AttachmentMeta, String> = upload_task.await;

        // 重新加锁，清理模拟状态并提交 Manifest
        let mut pending_ids = alloc_state.lock().await;

        // 无论上传成功或失败，必须擦除占用状态，防止死锁或序号永久丢失
        pending_ids.remove(&allocated_id);

        let attachment = upload_result?;

        // 更新 Manifest
        update_diary_attachment(&cache, &lfc, &crypto, &client, id, attachment.clone()).await?;

        let url = get_full_attachment_url(id, &attachment, &client)?;

        Ok::<(AttachmentMeta, String), String>((attachment, url))
    };

    match logic.await {
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e));
        }
        Ok((attachment, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(attachment, url));
        }
    }
}

pub async fn delete_attachment(
    cache: &DiaryMemoryCache,
    lfc: &LocalFileCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: String,
) -> Result<(), String> {
    let delete_lock = DELETE_LOCKS.entry(id.to_string()).or_default().clone();
    let _guard = delete_lock.lock().await;

    delete_diary_attachment(cache, lfc, crypto, client, id, &filename).await?;

    // 删除远端附件对象
    let attachment_key = remote_attachments_key(id, &filename);
    client
        .delete(&attachment_key)
        .await
        .map_err(|e| format!("Failed to delete attachment: {}", e))?;

    // 删除本地缓存文件
    lfc.delete(&attachment_key).await;

    Ok(())
}

pub async fn toggle_attachment_encryption(
    (crypto, cache, lfc, client): (Crypto, DiaryMemoryCache, LocalFileCache, OssClient),
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    filename: String,
) {
    let _ = event.send(AttachmentProcessEvent::Started);

    let logic = async {
        // 获取当前附件信息
        let diary = get_diary(&cache, &lfc, &crypto, &client, id).await?;
        let old_meta = diary
            .attachments
            .iter()
            .find(|a| a.filename == filename)
            .ok_or_else(|| "附件不存在".to_string())?
            .clone();

        // 我们需要反转当前的加密状态
        let encrypted = !old_meta.encrypted;
        let key = remote_attachments_key(id, &filename);

        // 下载原始数据：优先尝试从本地缓存获取
        let (raw_stream, size) = get_attachment_stream(&key, &lfc, &client, None).await?;

        // 处理流转换
        let (processed_stream, new_nonce) = if old_meta.encrypted && !encrypted {
            // 解密：从加密转为明文
            let decrypted = crypto.decrypt_streaming(raw_stream, &old_meta.nonce, 0)?;
            (decrypted, vec![])
        } else if !old_meta.encrypted && encrypted {
            // 加密：从明文转为加密
            let (encrypted_stream, nonce) = crypto.encrypt_streaming(raw_stream)?;
            (encrypted_stream, nonce)
        } else {
            return Err("无效的转换状态".to_string());
        };

        // 包装进度追踪
        let ec = event.clone();
        let tracked_stream = tracker_stream(size, processed_stream, move |p| {
            let _ = ec.send(AttachmentProcessEvent::Progress(p));
        });

        // 将新的状态包装进 lfc 缓存流并上传
        let (wrapped_stream, handle) = lfc.save(&key, tracked_stream).await?;

        match client
            .upload(&key, size, wrapped_stream, &old_meta.mimetype)
            .await
        {
            Ok(etag) => {
                let _ = handle.finalize(&etag).await;
            }
            Err(e) => {
                handle.abort().await;
                return Err(e);
            }
        }

        // 构造新的元数据并更新 Manifest
        let mut new_meta = old_meta.clone();
        new_meta.encrypted = encrypted;
        new_meta.nonce = new_nonce;

        update_diary_attachment(&cache, &lfc, &crypto, &client, id, new_meta.clone()).await?;

        let url = get_full_attachment_url(id, &new_meta, &client)?;
        Ok((new_meta, url))
    };

    match logic.await {
        Ok((meta, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(meta, url));
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e));
        }
    }
}

pub async fn rotate_image_attachment(
    (crypto, cache, lfc, client): (Crypto, DiaryMemoryCache, LocalFileCache, OssClient),
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    filename: String,
    rotation: i32,
) {
    let _ = event.send(AttachmentProcessEvent::Started);

    let logic = async {
        // 检测 rotation 参数是否合法
        if ![90, -90, 180].contains(&rotation) {
            return Err("不支持的旋转角度，仅支持 90, -90, 180".to_string());
        }
        // 获取元数据
        let diary = get_diary(&cache, &lfc, &crypto, &client, id).await?;
        let old_meta = diary
            .attachments
            .iter()
            .find(|a| a.filename == filename)
            .ok_or_else(|| "附件不存在".to_string())?
            .clone();

        // 验证 MIME 类型是否为图片
        if !old_meta.mimetype.starts_with("image/") {
            return Err("附件不是图片，无法旋转".to_string());
        }

        let key = remote_attachments_key(id, &filename);

        // 下载并解密原始数据：优先尝试从本地缓存获取
        let (raw_stream, _size) = get_attachment_stream(&key, &lfc, &client, None).await?;

        let stream = if old_meta.encrypted {
            crypto.decrypt_streaming(raw_stream, &old_meta.nonce, 0)?
        } else {
            raw_stream
        };

        // 将流收集到内存 图片处理必须在内存中进行
        let buffer = collect_data(stream).await?;

        // 使用 image 库处理旋转
        let img = image::load_from_memory(&buffer).map_err(|e| format!("图片解码失败: {}", e))?;

        let rotated_img = match rotation {
            90 => img.rotate90(),
            180 => img.rotate180(),
            -90 => img.rotate270(),
            _ => return Err("不支持的旋转角度，仅支持 90, 180, -90".to_string()),
        };

        // 4. 将旋转后的图片编码回字节流
        let mut output_buffer = Vec::new();
        let format = match old_meta.mimetype.as_str() {
            "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
            "image/png" => ImageFormat::Png,
            "image/webp" => ImageFormat::WebP,
            _ => ImageFormat::Png, // 默认 PNG
        };

        rotated_img
            .write_to(&mut Cursor::new(&mut output_buffer), format)
            .map_err(|e| format!("图片编码失败: {}", e))?;

        let new_size = output_buffer.len() as u64;

        // 重新上传并保持原有的加密策略
        let (upload_stream, new_nonce, is_encrypted) = if old_meta.encrypted {
            let (s, n) =
                crypto.encrypt_streaming(create_mock_stream(output_buffer, new_size as usize))?;
            (s, n, true)
        } else {
            (
                create_mock_stream(output_buffer, new_size as usize),
                vec![],
                false,
            )
        };

        // 更新上传并缓存
        let (wrapped_stream, handle) = lfc.save(&key, upload_stream).await?;
        match client
            .upload(&key, new_size, wrapped_stream, &old_meta.mimetype)
            .await
        {
            Ok(etag) => {
                let _ = handle.finalize(&etag).await;
            }
            Err(e) => {
                handle.abort().await;
                return Err(e);
            }
        }

        // 更新元数据
        let mut new_meta = old_meta.clone();
        new_meta.size = new_size;
        new_meta.nonce = new_nonce;
        new_meta.encrypted = is_encrypted;

        update_diary_attachment(&cache, &lfc, &crypto, &client, id, new_meta.clone()).await?;

        let url = get_full_attachment_url(id, &new_meta, &client)?;
        Ok((new_meta, url))
    };

    match logic.await {
        Ok((meta, url)) => {
            let _ = event.send(AttachmentProcessEvent::Completed(meta, url));
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e));
        }
    }
}

pub async fn caching_attachment(
    lfc: &LocalFileCache,
    client: &OssClient,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    filename: &str,
) {
    let _ = event.send(AttachmentProcessEvent::Started);
    let key = remote_attachments_key(id, filename);
    let logic = async {
        let metadata = client.get_metadata(&key).await?;
        if let Some(etag) = lfc.get(&key).await? {
            if metadata.etag() == etag {
                let _ = event.send(AttachmentProcessEvent::CompletedWithoutData);
                return Ok(());
            }
        }
        // 保存
        let (stream, len) = client.download(&key, None).await?;
        let ec = event.clone();
        let stream = tracker_stream(len, stream, move |progress| {
            let _ = ec.send(AttachmentProcessEvent::Progress(progress));
        });
        lfc.direct_save_with_md5(&key, metadata.etag(), stream)
            .await?;

        Ok::<(), String>(())
    };
    match logic.await {
        Ok(()) => {
            let _ = event.send(AttachmentProcessEvent::CompletedWithoutData);
        }
        Err(e) => {
            let _ = event.send(AttachmentProcessEvent::Error(e.clone()));
        }
    }
}

pub async fn save_decrypt_attachment(
    (crypto, _, lfc, client): (Crypto, DiaryMemoryCache, LocalFileCache, OssClient),
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: String,
    filename: String,
    attachment: AttachmentMeta,
    mut file: File,
) {
    let event_res_clone = event.clone();
    let _ = event.send(AttachmentProcessEvent::Started);
    let logic = async move {
        let key = remote_attachments_key(&id, &filename);
        let (stream, size) = get_attachment_stream(&key, &lfc, &client, None).await?;
        let event_clone = event.clone();
        let stream = tracker_stream(size, stream, move |p| {
            let _ = event_clone.send(AttachmentProcessEvent::Progress(p));
        });

        let mut stream = if attachment.encrypted {
            crypto.decrypt_streaming(stream, &attachment.nonce, 0)?
        } else {
            stream
        };

        while let Some(chunk) = stream.next().await {
            if let Ok(chunk) = chunk {
                file.write_all(&chunk)
                    .map_err(|e| format!("写入文件失败:{}", e))?;
            } else {
                return Err("下载文件失败".to_string());
            }
        }
        Ok::<(), String>(())
    };
    match logic.await {
        Err(e) => {
            let _ = event_res_clone.send(AttachmentProcessEvent::Error(e));
        }
        Ok(_) => {
            let _ = event_res_clone.send(AttachmentProcessEvent::CompletedWithoutData);
        }
    }
}

use crate::attachments::types::AttachmentProcessEvent;
use crate::attachments::{get_full_attachment_url, AttachmentMeta};
use crate::crypto::types::EncryptionAlgorithm::Ctr;
use crate::crypto::Crypto;
use crate::diaries::{
    delete_diary_attachment, get_diary, update_diary_attachment, DiaryMemoryCache,
};
use crate::object::OssClient;
use crate::storages::remote_attachments_key;
use crate::stream::tracker_stream::tracker_stream;
use crate::stream::ByteStream;
use crate::utils::create_mock_stream;
use crate::utils::message_sender::MessageSender;
use dashmap::DashMap;
use futures_util::StreamExt;
use image::ImageFormat;
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

// 添加附件的锁
static DIARY_ALLOCATORS: LazyLock<DashMap<String, Arc<Mutex<HashSet<u32>>>>> =
    LazyLock::new(DashMap::new);

// 删除附件的互斥锁
static DELETE_LOCKS: LazyLock<DashMap<String, Arc<Mutex<()>>>> = LazyLock::new(DashMap::new);

pub async fn add_attachment(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
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
            let diary = get_diary(&cache, &crypto, &client, id).await?;

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
            if !encrypted {
                client.upload(&key, size, stream, &mimetype).await?;
                Ok(AttachmentMeta {
                    filename,
                    mimetype,
                    size,
                    nonce: vec![], // 不加密时 nonce 为空
                    encrypted: false,
                    algorithm: Ctr,
                })
            } else {
                let (stream, nonce) = crypto.encrypt_streaming(stream)?;
                client.upload(&key, size, stream, &mimetype).await?;
                Ok(AttachmentMeta {
                    filename,
                    mimetype,
                    size,
                    nonce,
                    encrypted: true,
                    algorithm: Ctr,
                })
            }
        };

        let upload_result: Result<AttachmentMeta, String> = upload_task.await;

        // 重新加锁，清理模拟状态并提交 Manifest
        let mut pending_ids = alloc_state.lock().await;

        // 无论上传成功或失败，必须擦除占用状态，防止死锁或序号永久丢失
        pending_ids.remove(&allocated_id);

        let attachment = upload_result?;

        // 此时仍然持有 pending_ids 的锁，顺便当做排他锁更新 Manifest
        // 避免了并发上传完成后，同时回写 Manifest 导致的互相覆盖问题
        update_diary_attachment(&cache, &crypto, &client, id, attachment.clone()).await?;

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
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
    filename: String,
) -> Result<(), String> {
    let delete_lock = DELETE_LOCKS.entry(id.to_string()).or_default().clone();
    let _guard = delete_lock.lock().await;

    delete_diary_attachment(cache, crypto, client, id, &filename).await?;

    // 删除附件对象
    let attachment_key = remote_attachments_key(id, &filename);
    client
        .delete(&attachment_key)
        .await
        .map_err(|e| format!("Failed to delete attachment: {}", e))?;

    Ok(())
}

pub async fn toggle_attachment_encryption(
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
    event: Arc<dyn MessageSender<AttachmentProcessEvent>>,
    id: &str,
    filename: String,
    encrypted: bool,
) {
    let _ = event.send(AttachmentProcessEvent::Started);

    let logic = async {
        // 获取当前附件信息
        let diary = get_diary(&cache, &crypto, &client, id).await?;
        let old_meta = diary
            .attachments
            .iter()
            .find(|a| a.filename == filename)
            .ok_or_else(|| "附件不存在".to_string())?
            .clone();
        let key = remote_attachments_key(id, &filename);

        // 如果目标状态与当前状态一致，直接返回成功
        if old_meta.encrypted == encrypted {
            let url = get_full_attachment_url(id, &old_meta, &client)?;
            return Ok((old_meta, url));
        }

        // 下载原始数据
        let (raw_stream, size) = client
            .download(&remote_attachments_key(id, &filename), None)
            .await?;

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

        // 重新上传覆盖
        client
            .upload(&key, size, tracked_stream, &old_meta.mimetype)
            .await?;

        // 构造新的元数据并更新 Manifest
        let mut new_meta = old_meta.clone();
        new_meta.encrypted = encrypted;
        new_meta.nonce = new_nonce;

        update_diary_attachment(&cache, &crypto, &client, id, new_meta.clone()).await?;

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
    cache: DiaryMemoryCache,
    crypto: Crypto,
    client: OssClient,
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
        let diary = get_diary(&cache, &crypto, &client, id).await?;
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

        // 下载并解密原始数据
        let (raw_stream, _size) = client
            .download(&remote_attachments_key(id, &filename), None)
            .await?;

        let mut stream = if old_meta.encrypted {
            crypto.decrypt_streaming(raw_stream, &old_meta.nonce, 0)?
        } else {
            raw_stream
        };

        // 将流收集到内存 图片处理必须在内存中进行
        let mut buffer = Vec::new();
        while let Some(chunk) = stream.next().await {
            buffer.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        }

        // 使用 image 库处理旋转
        // load_from_memory 会自动识别 jpeg/png 等格式
        let img = image::load_from_memory(&buffer).map_err(|e| format!("图片解码失败: {}", e))?;

        let rotated_img = match rotation {
            90 => img.rotate90(),
            180 => img.rotate180(),
            -90 => img.rotate270(),
            _ => return Err("不支持的旋转角度，仅支持 90, 180, -90".to_string()),
        };

        // 4. 将旋转后的图片编码回字节流
        // 保持原始的 MIME 类型（简单起见，这里假设是常用格式）
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
        let key = remote_attachments_key(id, &filename);
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

        client
            .upload(&key, new_size, upload_stream, &old_meta.mimetype)
            .await?;

        // 更新元数据 (主要是 size 和可能的 nonce)
        let mut new_meta = old_meta.clone();
        new_meta.size = new_size;
        new_meta.nonce = new_nonce;
        new_meta.encrypted = is_encrypted;

        update_diary_attachment(&cache, &crypto, &client, id, new_meta.clone()).await?;

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

#[cfg(test)]
mod test {
    use super::*;
    use crate::diaries::{delete_diary, page_diary_ids, save_diary};
    use crate::utils::create_mock_stream;
    use futures::future::join_all;
    use serial_test::serial;
    use std::sync::Arc;
    use tokio::task::JoinHandle;

    #[serial]
    #[tokio::test]
    async fn test_thread_add_and_delete_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();

        // 0. 判断是是否为空
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("分页获取日记 ID 失败");
        assert!(
            ids.is_empty(),
            "测试前环境不干净，存在遗留日记数据，请清理后重试"
        );

        // 1. 预置数据: 初始化日记主体
        let (summary, _) = save_diary(&crypto, &client, "并发附件测试日记主体")
            .await
            .expect("未能初始化测试日记");
        let diary_id = summary.id;

        let concurrency_level = 10;
        let mut add_tasks: Vec<JoinHandle<_>> = Vec::with_capacity(concurrency_level);

        // 2. 核心测试: 并发添加附件
        for i in 0..concurrency_level {
            let cache_clone = cache.clone();
            let crypto_clone = crypto.clone();
            let client_clone = client.clone();
            let id_clone = diary_id.clone();
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

            // 构造 Mock 数据流 (需替换为项目实际的 ByteStream 构造方式)
            let dummy_content = format!("attachment_data_{}", i).into_bytes();
            let size = dummy_content.len() as u64;
            let stream = create_mock_stream(dummy_content, 1024);

            add_tasks.push(tokio::spawn(async move {
                add_attachment(
                    cache_clone,
                    crypto_clone,
                    client_clone,
                    Arc::new(tx),
                    &id_clone,
                    false,
                    size,
                    "text/plain".to_string(),
                    stream,
                )
                .await
            }));
        }

        // 等待所有并发写入完成
        let _ = join_all(add_tasks).await;

        // 3. 断言: 验证写一致性与防覆盖
        let manifest = get_diary(&cache, &crypto, &client, &diary_id)
            .await
            .expect("重新获取日记清单失败");

        assert_eq!(
            manifest.attachments.len(),
            concurrency_level,
            "并发写导致了附件元数据覆盖或丢失"
        );

        let mut filenames: Vec<u32> = manifest
            .attachments
            .iter()
            .filter_map(|a| a.filename.parse::<u32>().ok())
            .collect();
        filenames.sort_unstable();

        // 验证短文件名是否为 1 到 concurrency_level 的无间断严格递增序列
        let expected_filenames: Vec<u32> = (1..=concurrency_level as u32).collect();
        assert_eq!(
            filenames, expected_filenames,
            "并发环境下的短文件名分配出现冲突或跳号"
        );

        // 4. 核心测试: 并发删除附件
        let mut del_tasks: Vec<JoinHandle<_>> = Vec::with_capacity(concurrency_level);
        for filename in filenames {
            let cache_clone = cache.clone();
            let crypto_clone = crypto.clone();
            let client_clone = client.clone();
            let id_clone = diary_id.clone();
            let filename_str = filename.to_string();

            del_tasks.push(tokio::spawn(async move {
                delete_attachment(
                    &cache_clone,
                    &crypto_clone,
                    &client_clone,
                    &id_clone,
                    filename_str,
                )
                .await
            }));
        }

        let _ = join_all(del_tasks).await;

        // 5. 断言: 验证并发删一致性
        let final_manifest = get_diary(&cache, &crypto, &client, &diary_id)
            .await
            .expect("最终获取日记清单失败");

        assert!(
            final_manifest.attachments.is_empty(),
            "并发删除存在遗漏，附件未能全部清空"
        );

        // 清理测试数据
        delete_diary(&cache, &client, &diary_id)
            .await
            .expect("测试结束时清理日记失败");
    }

    #[serial]
    #[tokio::test]
    async fn test_toggle_attachment_encryption() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();

        // 预置数据：初始化日记
        let (summary, _) = save_diary(&crypto, &client, "加密切换测试")
            .await
            .expect("初始化日记失败");
        let diary_id = summary.id;

        // 上传一个明文附件
        let raw_data = b"hello encryption world".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        add_attachment(
            cache.clone(),
            crypto.clone(),
            client.clone(),
            Arc::new(tx),
            &diary_id,
            false, // 初始不加密
            size,
            "text/plain".to_string(),
            stream,
        )
        .await;

        let filename = "1"; // 第一个附件 ID 为 1

        // 切换为加密状态
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(
            cache.clone(),
            crypto.clone(),
            client.clone(),
            Arc::new(tx),
            &diary_id,
            filename.to_string(),
            true, // 开启加密
        )
        .await;

        // 验证元数据是否已更新为加密
        let diary_encrypted = get_diary(&cache, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta_enc = diary_encrypted.attachments.first().unwrap();
        assert!(meta_enc.encrypted, "附件应该是加密状态");
        assert!(!meta_enc.nonce.is_empty(), "加密状态下 nonce 不应为空");

        // 切换回明文状态
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(
            cache.clone(),
            crypto.clone(),
            client.clone(),
            Arc::new(tx),
            &diary_id,
            filename.to_string(),
            false, // 关闭加密
        )
        .await;

        // 检查数据是否还原
        let diary_decrypted = get_diary(&cache, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta_dec = diary_decrypted.attachments.first().unwrap();
        assert!(!meta_dec.encrypted, "附件应该是明文状态");
        assert!(meta_dec.nonce.is_empty(), "明文状态下 nonce 应该为空");

        // 下载并检查内容是否依然正确
        let (mut down_stream, _) = client
            .download(&remote_attachments_key(&diary_id, filename), None)
            .await
            .unwrap();
        let mut downloaded_bytes = Vec::new();
        use futures::StreamExt;
        while let Some(chunk) = down_stream.next().await {
            downloaded_bytes.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(downloaded_bytes, raw_data, "转换后的文件内容与原始数据不符");

        // 清理
        delete_diary(&cache, &client, &diary_id).await.unwrap();
    }

    #[serial]
    #[tokio::test]
    async fn test_rotate_image_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();

        // 准备环境：保存日记并上传一张原始图片
        let (summary, _) = save_diary(&crypto, &client, "图片旋转测试")
            .await
            .expect("初始化日记失败");
        let diary_id = summary.id;

        // 创建一个简单的 2x1 红色 RGBA 像素图片字节数据 (作为模拟)
        // 实际测试建议使用一个真实的 small jpeg 字节，或者用 image 库生成
        let mut img_buffer = Vec::new();
        let test_img = image::RgbImage::new(10, 20); // 宽10, 高20
        test_img
            .write_to(&mut Cursor::new(&mut img_buffer), ImageFormat::Png)
            .expect("生成测试图片失败");

        let original_size = img_buffer.len() as u64;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        // 上传原始图片
        add_attachment(
            cache.clone(),
            crypto.clone(),
            client.clone(),
            Arc::new(tx),
            &diary_id,
            true, // 测试加密状态下的旋转
            original_size,
            "image/png".to_string(),
            create_mock_stream(img_buffer, original_size as usize),
        )
        .await;

        let filename = "1";

        // 执行旋转操作：顺时针 90 度
        let (tx_rot, mut rx_rot) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        let event_sender = Arc::new(tx_rot);

        rotate_image_attachment(
            cache.clone(),
            crypto.clone(),
            client.clone(),
            event_sender,
            &diary_id,
            filename.to_string(),
            90, // 顺时针 90
        )
        .await;

        // 验证结果
        let mut completed = false;
        while let Some(event) = rx_rot.recv().await {
            match event {
                AttachmentProcessEvent::Started => {}
                AttachmentProcessEvent::Progress(_) => {}
                AttachmentProcessEvent::Completed(meta, _url) => {
                    assert_eq!(meta.filename, filename);
                    assert!(meta.encrypted, "旋转后应保持加密状态");
                    completed = true;
                }
                AttachmentProcessEvent::Error(e) => {
                    panic!("旋转失败: {}", e);
                }
            }
        }
        assert!(completed, "未收到 Completed 事件");

        // 验证数据确实被修改（下载并检查）
        let (raw_stream, _) = client
            .download(&remote_attachments_key(&diary_id, filename), None)
            .await
            .unwrap();

        // 解密
        let diary = get_diary(&cache, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta = diary.attachments.first().unwrap();
        let mut dec_stream = crypto
            .decrypt_streaming(raw_stream, &meta.nonce, 0)
            .unwrap();

        let mut rotated_data = Vec::new();
        use futures::StreamExt;
        while let Some(chunk) = dec_stream.next().await {
            rotated_data.extend_from_slice(&chunk.unwrap());
        }

        // 使用 image 库加载回旋转后的数据，验证尺寸是否互换 (10x20 -> 20x10)
        let final_img = image::load_from_memory(&rotated_data).expect("无法解码旋转后的图片");
        assert_eq!(final_img.width(), 20);
        assert_eq!(final_img.height(), 10);

        // 清理
        delete_diary(&cache, &client, &diary_id).await.unwrap();
    }
}

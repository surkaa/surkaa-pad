#[cfg(test)]
mod tests {
    use crate::attachments::attachment::{
        add_attachment, delete_attachment, rotate_image_attachment, toggle_attachment_encryption,
    };
    use crate::attachments::attachment_types::AttachmentProcessEvent;
    use crate::caches::{DiaryMemoryCache, LocalFileCache};
    use crate::cryptos::Crypto;
    use crate::diaries::{delete_diary, get_diary, page_diary_ids, save_diary};
    use crate::object::OssClient;
    use crate::storages::remote_attachments_key;
    use crate::stream::{collect_data, create_mock_stream};
    use futures::future::join_all;
    use image::ImageFormat;
    use serial_test::serial;
    use std::io::Cursor;
    use std::sync::Arc;
    use tokio::task::JoinHandle;

    #[serial]
    #[tokio::test]
    async fn test_thread_add_and_delete_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let lfc = LocalFileCache::new_test();

        // 0. 判断是是否为空
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("分页获取日记 ID 失败");
        assert!(
            ids.is_empty(),
            "测试前环境不干净，存在遗留日记数据，请清理后重试"
        );

        // 1. 预置数据: 初始化日记主体
        let (summary, _) = save_diary(&cache, &lfc, &crypto, &client, "并发附件测试日记主体")
            .await
            .expect("未能初始化测试日记");
        let diary_id = summary.id;

        let concurrency_level = 10;
        let mut add_tasks: Vec<JoinHandle<_>> = Vec::with_capacity(concurrency_level);

        // 2. 核心测试: 并发添加附件
        for i in 0..concurrency_level {
            let cache_clone = cache.clone();
            let lfc_clone = lfc.clone();
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
                    (crypto_clone, cache_clone, lfc_clone, client_clone),
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
        let manifest = get_diary(&cache, &lfc, &crypto, &client, &diary_id)
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
            let lfc_clone = lfc.clone();
            let crypto_clone = crypto.clone();
            let client_clone = client.clone();
            let id_clone = diary_id.clone();
            let filename_str = filename.to_string();

            del_tasks.push(tokio::spawn(async move {
                delete_attachment(
                    &cache_clone,
                    &lfc_clone,
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
        let final_manifest = get_diary(&cache, &lfc, &crypto, &client, &diary_id)
            .await
            .expect("最终获取日记清单失败");

        assert!(
            final_manifest.attachments.is_empty(),
            "并发删除存在遗漏，附件未能全部清空"
        );

        // 清理测试数据
        delete_diary(&cache, &lfc, &client, &diary_id)
            .await
            .expect("测试结束时清理日记失败");
    }

    #[serial]
    #[tokio::test]
    async fn test_toggle_attachment_encryption() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let lfc = LocalFileCache::new_test();

        // 预置数据：初始化日记
        let (summary, _) = save_diary(&cache, &lfc, &crypto, &client, "加密切换测试")
            .await
            .expect("初始化日记失败");
        let diary_id = summary.id;

        // 上传一个明文附件
        let raw_data = b"hello encryption world".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        add_attachment(
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
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
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
            Arc::new(tx),
            &diary_id,
            filename.to_string(),
        )
        .await;

        // 验证元数据是否已更新为加密
        let diary_encrypted = get_diary(&cache, &lfc, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta_enc = diary_encrypted.attachments.first().unwrap();
        assert!(meta_enc.encrypted, "附件应该是加密状态");
        assert!(!meta_enc.nonce.is_empty(), "加密状态下 nonce 不应为空");

        // 切换回明文状态
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
            Arc::new(tx),
            &diary_id,
            filename.to_string(),
        )
        .await;

        // 检查数据是否还原
        let diary_decrypted = get_diary(&cache, &lfc, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta_dec = diary_decrypted.attachments.first().unwrap();
        assert!(!meta_dec.encrypted, "附件应该是明文状态");
        assert!(meta_dec.nonce.is_empty(), "明文状态下 nonce 应该为空");

        // 下载并检查内容是否依然正确
        let (down_stream, _) = client
            .download(&remote_attachments_key(&diary_id, filename), None)
            .await
            .unwrap();
        let downloaded_bytes = collect_data(down_stream).await.expect("收集失败");
        assert_eq!(downloaded_bytes, raw_data, "转换后的文件内容与原始数据不符");

        // 清理
        delete_diary(&cache, &lfc, &client, &diary_id)
            .await
            .unwrap();
    }

    #[serial]
    #[tokio::test]
    async fn test_rotate_image_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let lfc = LocalFileCache::new_test();

        // 准备环境：保存日记并上传一张原始图片
        let (summary, _) = save_diary(&cache, &lfc, &crypto, &client, "图片旋转测试")
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
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
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
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
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
        let diary = get_diary(&cache, &lfc, &crypto, &client, &diary_id)
            .await
            .unwrap();
        let meta = diary.attachments.first().unwrap();
        let dec_stream = crypto
            .decrypt_streaming(raw_stream, &meta.nonce, 0)
            .unwrap();

        let rotated_data = collect_data(dec_stream).await.expect("收集旋转后数据失败");

        // 使用 image 库加载回旋转后的数据，验证尺寸是否互换 (10x20 -> 20x10)
        let final_img = image::load_from_memory(&rotated_data).expect("无法解码旋转后的图片");
        assert_eq!(final_img.width(), 20);
        assert_eq!(final_img.height(), 10);

        // 清理
        delete_diary(&cache, &lfc, &client, &diary_id)
            .await
            .unwrap();
    }

    #[serial]
    #[tokio::test]
    async fn test_attachment_local_cache_lifecycle() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let lfc = LocalFileCache::new_test();

        // 预置数据：初始化日记主体
        let (summary, _) = save_diary(&cache, &lfc, &crypto, &client, "附件缓存生命周期测试")
            .await
            .unwrap();
        let diary_id = summary.id;

        let raw_data = b"cache payload test".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        // 上传附件并测试缓存是否生成
        add_attachment(
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
            Arc::new(tx),
            &diary_id,
            false,
            size,
            "text/plain".to_string(),
            stream,
        )
        .await;

        let mut filename = String::new();
        while let Some(event) = rx.recv().await {
            if let AttachmentProcessEvent::Completed(m, _) = event {
                filename = m.filename;
                break;
            }
        }
        assert!(!filename.is_empty(), "附件上传未完成");

        let key = remote_attachments_key(&diary_id, &filename);

        // 验证缓存是否存在以及数据内容
        let cached = lfc.get(&key).await.unwrap();
        assert!(cached.is_some(), "附件上传后应该被正确缓存");
        let cached_data = lfc.get_data(&key).await.unwrap();
        assert_eq!(cached_data, raw_data, "本地缓存内容与上传的原始数据不一致");

        // 触发 toggle_attachment_encryption，验证缓存是否被正确替换
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(
            (crypto.clone(), cache.clone(), lfc.clone(), client.clone()),
            Arc::new(tx2),
            &diary_id,
            filename.clone(),
        )
        .await;

        while let Some(event) = rx2.recv().await {
            if let AttachmentProcessEvent::Completed(_, _) = event {
                break;
            }
        }

        // 验证切换加密后缓存内容已经发生变化（由于加入了加密，内容不可能与原文相同）
        let new_cached_data = lfc.get_data(&key).await.unwrap();
        assert_ne!(
            new_cached_data, raw_data,
            "切换加密后，本地缓存的载荷应该发生变化"
        );

        // 删除附件，验证缓存被清空
        delete_attachment(&cache, &lfc, &crypto, &client, &diary_id, filename)
            .await
            .unwrap();
        let cached_after_delete = lfc.get(&key).await.unwrap();
        assert!(
            cached_after_delete.is_none(),
            "附件删除后，关联的本地缓存应该被一并清除"
        );

        // 清理日记
        delete_diary(&cache, &lfc, &client, &diary_id)
            .await
            .unwrap();
    }
}

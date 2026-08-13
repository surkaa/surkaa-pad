#[cfg(test)]
mod tests {
    use crate::app_config::{AppConfig, AppConfigStore};
    use crate::attachments::attachment::{
        add_attachment, add_attachment_with_result, caching_attachment, delete_attachment,
        finish_attachment_replacement, rollback_attachment_replacement, rotate_image_attachment,
        toggle_attachment_encryption, update_attachment_filename,
    };
    use crate::attachments::attachment_types::AttachmentProcessEvent;
    use crate::caches::{AttachmentCacheManager, DiaryMemoryCache, LocalObjectStore};
    use crate::cryptos::Crypto;
    use crate::diaries::diary_store::{DiaryStore, LocalStore, RemoteStore};
    use crate::diaries::{delete_diary, get_diary, save_diary};
    use crate::object::OssClient;
    use crate::state::AppState;
    use crate::storages::remote_attachments_key;
    use crate::stream::{collect_data, create_mock_stream, ByteStream};
    use crate::test_utils::TestOssGuard;
    use bytes::Bytes;
    use futures::future::join_all;
    use futures_util::stream;
    use image::ImageFormat;
    use std::io::Cursor;
    use std::io::Error;
    use std::sync::Arc;
    use tokio::task::JoinHandle;

    fn make_remote_state(crypto: &Crypto, client: &OssClient, los: &LocalObjectStore) -> AppState {
        let state = AppState::from_parts(crypto.clone(), client.clone(), los.clone());
        state.set_remote_enabled(true);
        state
    }

    #[tokio::test]
    async fn standard_upload_rolls_back_when_diary_is_deleted_before_manifest_update() {
        let crypto = Crypto::from_env();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let los = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let state = AppState::from_parts(crypto.clone(), OssClient::new(), los.clone());
        let store = state.diary_store();
        let (summary, _) = save_diary(&state.diary_cache(), &crypto, &*store, "upload race")
            .await
            .unwrap();

        let (stream_polled_tx, stream_polled_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let upload_stream: ByteStream = Box::pin(stream::once(async move {
            let _ = stream_polled_tx.send(());
            release_rx.await.expect("release upload stream");
            Ok::<_, Error>(Bytes::from_static(b"pending attachment"))
        }));
        let upload_state = state.clone();
        let diary_id = summary.id.clone();
        let upload = tokio::spawn(async move {
            let (event, _rx) = tokio::sync::mpsc::unbounded_channel();
            add_attachment_with_result(
                &upload_state,
                Arc::new(event),
                &diary_id,
                false,
                18,
                "text/plain".to_string(),
                upload_stream,
                Some("pending.txt".to_string()),
            )
            .await
        });

        stream_polled_rx
            .await
            .expect("upload stream was not polled");
        let store = state.diary_store();
        delete_diary(&state.diary_cache(), &*store, &summary.id)
            .await
            .unwrap();
        release_tx.send(()).unwrap();

        assert!(upload.await.unwrap().is_err());
        assert!(los.get_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn encryption_replacement_restores_old_object_when_manifest_publish_fails() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let los = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let store = LocalStore::new(los);
        let old_data = b"readable before metadata update".to_vec();
        let replacement = b"encrypted replacement bytes".to_vec();

        store
            .upload_attachment(
                "diary",
                "attachment",
                old_data.len() as u64,
                "text/plain",
                create_mock_stream(old_data.clone(), old_data.len()),
            )
            .await
            .unwrap();
        store
            .create_attachment_backup("diary", "attachment")
            .await
            .unwrap();
        store
            .upload_attachment(
                "diary",
                "attachment",
                replacement.len() as u64,
                "text/plain",
                create_mock_stream(replacement.clone(), replacement.len()),
            )
            .await
            .unwrap();

        let result =
            finish_attachment_replacement(&store, "diary", "attachment", "text/plain", || async {
                Err(crate::attachments::AttachmentError::InvalidOperation(
                    "manifest publish failed".to_string(),
                ))
            })
            .await;

        assert!(result.is_err());
        let stream = store
            .download_attachment("diary", "attachment", None, None)
            .await
            .unwrap();
        assert_eq!(collect_data(stream).await.unwrap(), old_data);
        assert!(store
            .restore_attachment_backup("diary", "attachment", "text/plain")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn encryption_replacement_restores_old_object_when_upload_reports_failure() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let old_data = b"old attachment remains recoverable".to_vec();
        let replacement = b"replacement committed before failure".to_vec();

        store
            .upload_attachment(
                "diary",
                "attachment",
                old_data.len() as u64,
                "text/plain",
                create_mock_stream(old_data.clone(), old_data.len()),
            )
            .await
            .unwrap();
        store
            .create_attachment_backup("diary", "attachment")
            .await
            .unwrap();
        store
            .upload_attachment(
                "diary",
                "attachment",
                replacement.len() as u64,
                "text/plain",
                create_mock_stream(replacement.clone(), replacement.len()),
            )
            .await
            .unwrap();

        let error = rollback_attachment_replacement(
            &store,
            "diary",
            "attachment",
            "text/plain",
            crate::attachments::AttachmentError::InvalidOperation(
                "upload finalization failed".to_string(),
            ),
        )
        .await;

        assert!(error.to_string().contains("upload finalization failed"));
        let stream = store
            .download_attachment("diary", "attachment", None, None)
            .await
            .unwrap();
        assert_eq!(collect_data(stream).await.unwrap(), old_data);
        assert!(store
            .restore_attachment_backup("diary", "attachment", "text/plain")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn remote_encryption_replacement_restores_old_object_after_failure() {
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let los = LocalObjectStore::new(temp_dir.path().to_path_buf());
        let config = AppConfigStore::in_memory(AppConfig::with_attachment_cache_limit_bytes(100));
        config.set_attachment_cache_max_file_size_bytes(4).unwrap();
        let attachment_cache = AttachmentCacheManager::new(los.clone(), config);
        let store = RemoteStore::with_attachment_cache(los.clone(), client, attachment_cache);
        let diary_id = "8215021834823";
        let old_data = b"old remote attachment".to_vec();
        let replacement = b"new remote attachment".to_vec();

        store
            .upload_attachment(
                diary_id,
                "attachment",
                old_data.len() as u64,
                "text/plain",
                create_mock_stream(old_data.clone(), old_data.len()),
            )
            .await
            .unwrap();
        assert!(los
            .get(&remote_attachments_key(diary_id, "attachment"))
            .await
            .unwrap()
            .is_none());
        store
            .create_attachment_backup(diary_id, "attachment")
            .await
            .unwrap();
        store
            .upload_attachment(
                diary_id,
                "attachment",
                replacement.len() as u64,
                "text/plain",
                create_mock_stream(replacement.clone(), replacement.len()),
            )
            .await
            .unwrap();

        rollback_attachment_replacement(
            &store,
            diary_id,
            "attachment",
            "text/plain",
            crate::attachments::AttachmentError::InvalidOperation(
                "remote upload finalization failed".to_string(),
            ),
        )
        .await;

        let stream = store
            .download_attachment(diary_id, "attachment", None, None)
            .await
            .unwrap();
        assert_eq!(collect_data(stream).await.unwrap(), old_data);
        assert!(los
            .get(&remote_attachments_key(diary_id, "attachment"))
            .await
            .unwrap()
            .is_none());
        assert!(store
            .restore_attachment_backup(diary_id, "attachment", "text/plain")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_thread_add_and_delete_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());

        // 预置数据: 初始化日记主体
        let (summary, _) = save_diary(&cache, &crypto, &store, "并发附件测试日记主体")
            .await
            .expect("未能初始化测试日记");
        let diary_id = summary.id;

        let concurrency_level = 10;
        let mut add_tasks: Vec<JoinHandle<_>> = Vec::with_capacity(concurrency_level);

        let state = make_remote_state(&crypto, &client, &los);

        // 2. 核心测试: 并发添加附件
        for i in 0..concurrency_level {
            let state = state.clone();
            let id_clone = diary_id.clone();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

            // 构造 Mock 数据流 (需替换为项目实际的 ByteStream 构造方式)
            let dummy_content = format!("attachment_data_{}", i).into_bytes();
            let size = dummy_content.len() as u64;
            let stream = create_mock_stream(dummy_content, 1024);

            add_tasks.push(tokio::spawn(async move {
                let result = add_attachment_with_result(
                    &state,
                    Arc::new(tx),
                    &id_clone,
                    false,
                    size,
                    "text/plain".to_string(),
                    stream,
                    None,
                )
                .await;
                drop(rx);
                result
            }));
        }

        // 等待所有并发写入完成
        for result in join_all(add_tasks).await {
            result
                .expect("附件上传任务 panic")
                .expect("附件上传或 Manifest 更新失败");
        }

        // 3. 断言: 验证写一致性与防覆盖
        let store = RemoteStore::new(los.clone(), client.clone());
        let manifest = get_diary(&cache, &crypto, &store, &diary_id)
            .await
            .expect("重新获取日记清单失败");

        assert_eq!(
            manifest.attachments.len(),
            concurrency_level,
            "并发写导致了附件元数据覆盖或丢失"
        );

        let attachment_ids: Vec<String> = manifest
            .attachments
            .iter()
            .map(|attachment| attachment.id.clone())
            .collect();
        assert_eq!(
            attachment_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            concurrency_level,
            "并发环境下生成了重复附件 ID"
        );
        assert!(attachment_ids.iter().all(|id| id.starts_with("att-")));
        assert_eq!(
            manifest
                .attachments
                .iter()
                .map(|attachment| &attachment.filename)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            concurrency_level,
            "并发环境下展示文件名未正确去重"
        );

        // 4. 核心测试: 并发删除附件
        let mut del_tasks: Vec<JoinHandle<_>> = Vec::with_capacity(concurrency_level);
        for attachment_id in attachment_ids {
            let cache_clone = cache.clone();
            let crypto_clone = crypto.clone();
            let los_clone = los.clone();
            let client_clone = client.clone();
            let id_clone = diary_id.clone();
            del_tasks.push(tokio::spawn(async move {
                let store = RemoteStore::new(los_clone, client_clone);
                delete_attachment(
                    &cache_clone,
                    &crypto_clone,
                    &store,
                    &id_clone,
                    attachment_id,
                )
                .await
            }));
        }

        for result in join_all(del_tasks).await {
            result
                .expect("附件删除任务 panic")
                .expect("附件删除或 Manifest 更新失败");
        }

        // 5. 断言: 验证并发删一致性
        let store = RemoteStore::new(los.clone(), client.clone());
        let final_manifest = get_diary(&cache, &crypto, &store, &diary_id)
            .await
            .expect("最终获取日记清单失败");

        assert!(
            final_manifest.attachments.is_empty(),
            "并发删除存在遗漏，附件未能全部清空"
        );
        _guard.cleanup().await;
    }

    #[tokio::test]
    async fn test_toggle_attachment_encryption() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());
        let state = make_remote_state(&crypto, &client, &los);

        // 预置数据：初始化日记
        let (summary, _) = save_diary(&cache, &crypto, &store, "加密切换测试")
            .await
            .expect("初始化日记失败");
        let diary_id = summary.id;

        // 上传一个明文附件
        let raw_data = b"hello encryption world".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        add_attachment(
            &state,
            Arc::new(tx),
            &diary_id,
            false, // 初始不加密
            size,
            "text/plain".to_string(),
            stream,
            None,
        )
        .await;

        let attachment_id = get_diary(&cache, &crypto, &store, &diary_id)
            .await
            .unwrap()
            .attachments[0]
            .id
            .clone();

        // 切换为加密状态
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(&state, Arc::new(tx), &diary_id, attachment_id.clone()).await;

        // 验证元数据是否已更新为加密
        let store = RemoteStore::new(los.clone(), client.clone());
        let diary_encrypted = get_diary(&cache, &crypto, &store, &diary_id).await.unwrap();
        let meta_enc = diary_encrypted.attachments.first().unwrap();
        assert!(meta_enc.encrypted, "附件应该是加密状态");
        assert!(!meta_enc.nonce.is_empty(), "加密状态下 nonce 不应为空");

        // 切换回明文状态
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(&state, Arc::new(tx), &diary_id, attachment_id.clone()).await;

        // 检查数据是否还原
        let store = RemoteStore::new(los.clone(), client.clone());
        let diary_decrypted = get_diary(&cache, &crypto, &store, &diary_id).await.unwrap();
        let meta_dec = diary_decrypted.attachments.first().unwrap();
        assert!(!meta_dec.encrypted, "附件应该是明文状态");
        assert!(meta_dec.nonce.is_empty(), "明文状态下 nonce 应该为空");

        // 下载并检查内容是否依然正确
        let (down_stream, _) = client
            .download(&remote_attachments_key(&diary_id, &attachment_id), None)
            .await
            .unwrap();
        let downloaded_bytes = collect_data(down_stream).await.expect("收集失败");
        assert_eq!(downloaded_bytes, raw_data, "转换后的文件内容与原始数据不符");
        _guard.cleanup().await;
    }

    #[tokio::test]
    async fn test_rotate_image_attachment() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());
        let state = make_remote_state(&crypto, &client, &los);

        // 准备环境：保存日记并上传一张原始图片
        let (summary, _) = save_diary(&cache, &crypto, &store, "图片旋转测试")
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
            &state,
            Arc::new(tx),
            &diary_id,
            true, // 测试加密状态下的旋转
            original_size,
            "image/png".to_string(),
            create_mock_stream(img_buffer, original_size as usize),
            None,
        )
        .await;

        let attachment_id = get_diary(&cache, &crypto, &store, &diary_id)
            .await
            .unwrap()
            .attachments[0]
            .id
            .clone();

        // 执行旋转操作：顺时针 90 度
        let (tx_rot, mut rx_rot) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        let event_sender = Arc::new(tx_rot);

        rotate_image_attachment(
            &state,
            event_sender,
            &diary_id,
            attachment_id.clone(),
            90, // 顺时针 90
        )
        .await;

        // 验证结果
        let mut completed = false;
        while let Some(event) = rx_rot.recv().await {
            match event {
                AttachmentProcessEvent::Started => {}
                AttachmentProcessEvent::Progress(_) => {}
                AttachmentProcessEvent::Finalizing => {}
                AttachmentProcessEvent::Completed(meta, _url) => {
                    assert_eq!(meta.id, attachment_id);
                    assert!(meta.encrypted, "旋转后应保持加密状态");
                    completed = true;
                }
                AttachmentProcessEvent::CompletedWithoutData => {
                    panic!("旋转不应该出现这个状态")
                }
                AttachmentProcessEvent::Error(e) => {
                    panic!("旋转失败: {}", e);
                }
            }
        }
        assert!(completed, "未收到 Completed 事件");

        // 验证数据确实被修改（下载并检查）
        let (raw_stream, _) = client
            .download(&remote_attachments_key(&diary_id, &attachment_id), None)
            .await
            .unwrap();

        // 解密
        let store = RemoteStore::new(los.clone(), client.clone());
        let diary = get_diary(&cache, &crypto, &store, &diary_id).await.unwrap();
        let meta = diary.attachments.first().unwrap();
        let dec_stream = crypto
            .decrypt_streaming(raw_stream, &meta.nonce, 0)
            .unwrap();

        let rotated_data = collect_data(dec_stream).await.expect("收集旋转后数据失败");

        // 使用 image 库加载回旋转后的数据，验证尺寸是否互换 (10x20 -> 20x10)
        let final_img = image::load_from_memory(&rotated_data).expect("无法解码旋转后的图片");
        assert_eq!(final_img.width(), 20);
        assert_eq!(final_img.height(), 10);
        _guard.cleanup().await;
    }

    #[tokio::test]
    async fn test_attachment_local_cache_lifecycle() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());
        let state = make_remote_state(&crypto, &client, &los);

        // 预置数据：初始化日记主体
        let (summary, _) = save_diary(&cache, &crypto, &store, "附件缓存生命周期测试")
            .await
            .unwrap();
        let diary_id = summary.id;

        let raw_data = b"cache payload test".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();

        // 上传附件并测试缓存是否生成
        add_attachment(
            &state,
            Arc::new(tx),
            &diary_id,
            false,
            size,
            "text/plain".to_string(),
            stream,
            None,
        )
        .await;

        let mut attachment_id = String::new();
        while let Some(event) = rx.recv().await {
            if let AttachmentProcessEvent::Completed(m, _) = event {
                attachment_id = m.id;
                break;
            }
        }
        assert!(!attachment_id.is_empty(), "附件上传未完成");

        let key = remote_attachments_key(&diary_id, &attachment_id);

        // 验证缓存是否存在以及数据内容
        let cached = los.get(&key).await.unwrap();
        assert!(cached.is_some(), "附件上传后应该被正确缓存");
        let cached_data = los.get_data(&key).await.unwrap();
        assert_eq!(cached_data, raw_data, "本地缓存内容与上传的原始数据不一致");

        // 删除已有缓存后主动缓存，验证远端附件被完整下载并只发送一次完成事件。
        los.delete(&key).await.unwrap();
        assert!(los.get(&key).await.unwrap().is_none());

        let (cache_tx, mut cache_rx) =
            tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        caching_attachment(&store, Arc::new(cache_tx), &diary_id, &attachment_id).await;

        let mut started_count = 0;
        let mut completed_count = 0;
        let mut reached_100_percent = false;
        while let Ok(event) = cache_rx.try_recv() {
            match event {
                AttachmentProcessEvent::Started => started_count += 1,
                AttachmentProcessEvent::Progress(progress) => {
                    reached_100_percent |= progress == 100;
                }
                AttachmentProcessEvent::Finalizing => {
                    panic!("主动缓存不应进入附件上传完成阶段")
                }
                AttachmentProcessEvent::CompletedWithoutData => completed_count += 1,
                AttachmentProcessEvent::Error(error) => {
                    panic!("主动缓存附件失败: {error}")
                }
                AttachmentProcessEvent::Completed(_, _) => {
                    panic!("主动缓存不应返回附件元数据")
                }
            }
        }
        assert_eq!(started_count, 1);
        assert_eq!(completed_count, 1);
        assert!(reached_100_percent);
        assert_eq!(los.get_data(&key).await.unwrap(), raw_data);

        // 触发 toggle_attachment_encryption，验证缓存是否被正确替换
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        toggle_attachment_encryption(&state, Arc::new(tx2), &diary_id, attachment_id.clone()).await;

        while let Some(event) = rx2.recv().await {
            if let AttachmentProcessEvent::Completed(_, _) = event {
                break;
            }
        }

        // 验证切换加密后缓存内容已经发生变化（由于加入了加密，内容不可能与原文相同）
        let new_cached_data = los.get_data(&key).await.unwrap();
        assert_ne!(
            new_cached_data, raw_data,
            "切换加密后，本地缓存的载荷应该发生变化"
        );

        // 删除附件，验证缓存被清空
        let store = RemoteStore::new(los.clone(), client.clone());
        delete_attachment(&cache, &crypto, &store, &diary_id, attachment_id)
            .await
            .unwrap();
        let cached_after_delete = los.get(&key).await.unwrap();
        assert!(
            cached_after_delete.is_none(),
            "附件删除后，关联的本地缓存应该被一并清除"
        );
        _guard.cleanup().await;
    }

    #[tokio::test]
    async fn test_attachment_rename() {
        let cache = DiaryMemoryCache::new();
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());
        let state = make_remote_state(&crypto, &client, &los);
        let (summary, _) = save_diary(&cache, &crypto, &store, "test-content")
            .await
            .expect("初始化日记失败");
        let diary_id = summary.id;
        let raw_data = b"cache payload test".to_vec();
        let size = raw_data.len() as u64;
        let stream = create_mock_stream(raw_data.clone(), size as usize);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AttachmentProcessEvent>();
        add_attachment(
            &state,
            Arc::new(tx),
            &diary_id,
            false,
            size,
            "text/plain".to_string(),
            stream,
            None,
        )
        .await;
        let mut attachment = None;
        while let Some(event) = rx.recv().await {
            match event {
                AttachmentProcessEvent::Started => {}
                AttachmentProcessEvent::Progress(_) => {}
                AttachmentProcessEvent::Finalizing => {}
                AttachmentProcessEvent::Completed(m, _) => attachment = Some(m),
                AttachmentProcessEvent::CompletedWithoutData => {}
                AttachmentProcessEvent::Error(e) => panic!("添加附件失败: {}", e),
            }
        }
        let attachment = attachment.expect("附件上传未完成");
        let new_filename = "test";
        // 更名
        update_attachment_filename(
            &state,
            &diary_id,
            attachment.id.clone(),
            new_filename.to_string(),
        )
        .await
        .expect("附件更名失败");
        // 检查
        let store = RemoteStore::new(los.clone(), client.clone());
        let diary = get_diary(&cache, &crypto, &store, &diary_id)
            .await
            .expect("获取日记失败");
        let meta = diary.attachments.first().unwrap();
        assert_eq!(meta.filename, new_filename, "附件更名后元数据未更新");
        assert_eq!(meta.id, attachment.id, "重命名不应改变附件 ID");
        let (stream, _) = client
            .download(&remote_attachments_key(&diary_id, &attachment.id), None)
            .await
            .expect("重命名后原附件 ID 对象应该仍然存在");
        assert_eq!(collect_data(stream).await.unwrap(), raw_data);
        _guard.cleanup().await;
    }
}

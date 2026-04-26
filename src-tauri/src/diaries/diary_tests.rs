#[cfg(test)]
mod diary_tests {
    use crate::caches::{DiaryMemoryCache, LocalFileCache};
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::cryptos::Crypto;
    use crate::diaries::diary_migration::CURRENT_VERSION;
    use crate::diaries::diary_types::DiaryManifest;
    use crate::diaries::{delete_diary, get_diary, save_diary, update_diary_content_only};
    use crate::object::OssClient;
    use crate::storages::remote_manifest_key;
    use chrono::Utc;
    use serde_json::to_vec;
    use serial_test::serial;
    use std::time::Duration;

    #[serial]
    #[tokio::test]
    async fn test_diary_crud_lifecycle() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let lfc = LocalFileCache::new(path);

        // 判断为空，确保测试环境干净
        let (objects, _) = client.list("", None).await.expect("未能列出对象");
        assert!(
            objects.is_empty(),
            "测试环境不干净。请确保运行测试前OSS桶是空的。"
        );

        // 测试创建
        let initial_content = "Integration test diary content.";
        let (summary, content) = save_diary(&cache, &lfc, &crypto, &client, initial_content)
            .await
            .expect("未能保存日记");

        assert_eq!(content, initial_content);
        assert!(!summary.id.is_empty());
        let id = summary.id.clone();

        // 测试读取 - 验证远端拉取并写入缓存
        let fetched_manifest = get_diary(&cache, &lfc, &crypto, &client, &id)
            .await
            .expect("远程获取日记失败");

        assert_eq!(fetched_manifest.id, id);
        assert_eq!(fetched_manifest.content, initial_content);

        // 为了确保 update 生成的时间戳严格大于前一次，休眠防 Flaky Test
        tokio::time::sleep(Duration::from_millis(5)).await;

        // 测试更新
        let updated_content = "Updated content for testing.";
        let updated_summary =
            update_diary_content_only(&cache, &lfc, &crypto, &client, &id, updated_content)
                .await
                .expect("未能更新日记");

        assert!(updated_summary.updated > summary.updated);

        // 测试再次读取 - 验证缓存失效/更新机制
        let refetched_manifest = get_diary(&cache, &lfc, &crypto, &client, &id)
            .await
            .expect("未能重新获取更新的日记");

        assert_eq!(refetched_manifest.content, updated_content);

        // 测试删除
        delete_diary(&cache, &lfc, &client, &id)
            .await
            .expect("删除日记失败");

        // 验证删除有效性
        let not_found_result = get_diary(&cache, &lfc, &crypto, &client, &id).await;
        assert!(not_found_result.is_err(), "删除后日记不应被检索");
    }

    #[serial]
    #[tokio::test]
    async fn test_local_file_cache_integration() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let lfc = LocalFileCache::new(path);

        // 保存第一篇日记
        let content1 = "Original content for cache test.";
        let (summary, _) = save_diary(&cache, &lfc, &crypto, &client, content1)
            .await
            .expect("保存日记失败");
        let id = summary.id.clone();
        let object_key = remote_manifest_key(&id);

        // 获取 OSS 上的 etag
        let metadata = client
            .get_metadata(&object_key)
            .await
            .expect("获取元数据失败");
        let etag1 = metadata.etag.unwrap_or_default().to_string();

        // 验证本地缓存已生成
        let cached = lfc.get(&object_key).await.expect("检查缓存失败");
        assert!(cached.is_some(), "本地缓存文件未生成");
        let cached_etag1 = cached.unwrap();
        assert_eq!(cached_etag1, etag1, "本地缓存的 etag 与 OSS 不一致");

        // 模拟外部直接修改 OSS 上的日记内容（绕过更新接口）
        // 构造一个新的 manifest，内容不同，id 相同
        let modified_manifest = DiaryManifest {
            id: id.clone(),
            algorithm: Gcm,
            content: "Modified content after external update.".to_string(),
            created: summary.created,
            updated: Utc::now().timestamp_millis(),
            attachments: Vec::new(),
            version: CURRENT_VERSION,
        };
        let manifest_json = to_vec(&modified_manifest).expect("序列化失败");
        let encrypted_modified = crypto.encrypt(&manifest_json).expect("加密失败");
        let new_etag = client
            .upload_bytes(&object_key, &encrypted_modified)
            .await
            .expect("上传修改后的日记失败");

        // 确保 etag 已变化
        assert_ne!(new_etag, etag1, "外部修改后 etag 应发生变化");

        // 清空内存缓存（新建实例）
        let cache2 = DiaryMemoryCache::new();

        // 再次获取日记，此时应因本地缓存 etag 不匹配而重新下载
        let fetched = get_diary(&cache2, &lfc, &crypto, &client, &id)
            .await
            .expect("获取日记失败");
        assert_eq!(
            fetched.content, modified_manifest.content,
            "获取到的内容应为修改后的内容"
        );

        // 验证本地缓存已被更新为新 etag 和新内容
        let cached_after = lfc.get(&object_key).await.expect("检查缓存失败");
        assert!(cached_after.is_some(), "本地缓存应存在");
        let cached_etag2 = cached_after.unwrap();
        assert_eq!(cached_etag2, new_etag, "本地缓存的 etag 未更新");

        // 验证本地缓存文件解密后的内容是否正确
        let cached_bytes = lfc.get_data(&object_key).await.expect("读取缓存数据失败");
        let decrypted = crypto.decrypt(&cached_bytes).expect("解密缓存数据失败");
        let cached_manifest: DiaryManifest =
            serde_json::from_slice(&decrypted).expect("反序列化缓存数据失败");
        assert_eq!(
            cached_manifest.content, modified_manifest.content,
            "本地缓存文件内容与预期不符"
        );

        // 测试删除日记时本地缓存是否被清理
        delete_diary(&cache2, &lfc, &client, &id)
            .await
            .expect("删除日记失败");
        let cached_after_delete = lfc.get(&object_key).await.expect("检查缓存失败");
        assert!(cached_after_delete.is_none(), "删除后本地缓存应被移除");
    }
}

#[cfg(test)]
mod diary_list_tests {
    use crate::caches::{DiaryMemoryCache, LocalFileCache};
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{delete_diary, save_diary};
    use crate::diaries::{get_diary_content, get_diary_summary, page_diary_ids};
    use crate::object::OssClient;
    use serial_test::serial;

    #[serial]
    #[tokio::test]
    async fn test_diary_list() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let lfc = LocalFileCache::new(path);

        // 确保是空的测试环境
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("无法获取日记列表");
        assert!(ids.is_empty(), "测试环境不干净，存在日记数据");

        // 创建几个测试日记
        let title = "这是一个测试日记的标题";
        let content = "这是一个测试日记内容";
        let test_count = 21; // 测试环境下按10条分页，创建21条以测试分页逻辑
        for _ in 0..test_count {
            let _ = save_diary(
                &cache,
                &lfc,
                &crypto,
                &client,
                format!("{}\n{}", title, content).as_str(),
            )
            .await
            .expect("无法保存日记");
        }

        // 列出日记ID
        let mut next_token = None;
        let mut all_ids = Vec::new();
        let mut page_count = 0;
        loop {
            let (ids, nt) = page_diary_ids(&client, next_token)
                .await
                .expect("无法获取日记列表");
            all_ids.extend(ids);
            page_count += 1;
            if nt.is_none() {
                break;
            }
            next_token = nt;
        }

        // 验证总数和内容
        assert_eq!(all_ids.len(), test_count);
        assert_eq!(page_count, 3, "分页逻辑错误，预期3页但实际{}", page_count);
        for id in all_ids.clone() {
            let summary = get_diary_summary(&cache, &lfc, &crypto, &client, &id)
                .await
                .expect("无法获取日记摘要");
            assert_eq!(summary.title, title);
            let content = get_diary_content(&cache, &lfc, &crypto, &client, &id)
                .await
                .expect("无法获取日记内容");
            assert_eq!(content, content);
        }

        // 清理测试数据
        for id in all_ids {
            let _ = delete_diary(&cache, &lfc, &client, &id)
                .await
                .expect("无法删除测试日记");
        }
    }
}

#[cfg(test)]
mod diary_search_tests {
    use crate::caches::{DiaryMemoryCache, LocalFileCache};
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{delete_diary, save_diary};
    use crate::diaries::diary_search::search_diaries;
    use crate::diaries::diary_types::{DiarySummary, SearchDiariesEvent};
    use crate::diaries::page_diary_ids;
    use crate::object::OssClient;
    use serial_test::serial;
    use std::sync::Arc;

    async fn test_search(
        cache: &DiaryMemoryCache,
        lfc: &LocalFileCache,
        crypto: &Crypto,
        client: &OssClient,
        keyword: String,
        or: bool,
    ) -> (Vec<DiarySummary>, Vec<String>) {
        // 创建事件监听器
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SearchDiariesEvent>();
        let event_sender = Arc::new(tx);
        let _ = search_diaries(
            &cache,
            &lfc,
            &crypto,
            &client,
            event_sender.clone(),
            keyword,
            or,
        )
        .await;

        let mut matches = Vec::new();
        let mut unmatches = Vec::new();
        let mut finished = false;
        let mut error = None;

        while let Some(event) = rx.recv().await {
            match event {
                SearchDiariesEvent::Match(summary) => matches.push(summary),
                SearchDiariesEvent::Unmatch(id) => unmatches.push(id),
                SearchDiariesEvent::Finished => {
                    finished = true;
                    break;
                }
                SearchDiariesEvent::Error(e) => {
                    error = Some(e);
                    break;
                }
            }
        }

        assert!(error.is_none(), "搜索过程中发生错误: {:?}", error);
        assert!(finished, "搜索未正常完成");

        (matches, unmatches)
    }

    #[serial]
    #[tokio::test]
    async fn test_diary_search() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let lfc = LocalFileCache::new(path);

        // 确保是空的测试环境
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("无法获取日记列表");
        assert!(ids.is_empty(), "测试环境不干净，存在日记数据");

        // 创建几个测试日记
        let _ = save_diary(
            &cache,
            &lfc,
            &crypto,
            &client,
            "这是第一篇日记，包含关键词 rust",
        )
        .await;
        let _ = save_diary(
            &cache,
            &lfc,
            &crypto,
            &client,
            "这是第二篇日记，不包含关键词",
        )
        .await;
        let _ = save_diary(
            &cache,
            &lfc,
            &crypto,
            &client,
            "这是第三篇日记，包含关键词 rust 和 async",
        )
        .await;
        let _ = save_diary(
            &cache,
            &lfc,
            &crypto,
            &client,
            "这是第四篇日记，包含关键词 async",
        )
        .await;

        // 收集结果
        let (matches, unmatches) =
            test_search(&cache, &lfc, &crypto, &client, "rust".to_string(), true).await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'rust' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'rust' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) =
            test_search(&cache, &lfc, &crypto, &client, "async".to_string(), true).await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'async' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'async' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) = test_search(
            &cache,
            &lfc,
            &crypto,
            &client,
            "rust async".to_string(),
            false,
        )
        .await;
        assert_eq!(
            matches.len(),
            1,
            "使用 AND 搜索 'rust async' 应该匹配 1 篇日记"
        );
        assert_eq!(
            unmatches.len(),
            3,
            "使用 AND 搜索 'rust async' 应该不匹配 3 篇日记"
        );

        let (matches, unmatches) = test_search(
            &cache,
            &lfc,
            &crypto,
            &client,
            "rust async".to_string(),
            true,
        )
        .await;
        assert_eq!(
            matches.len(),
            3,
            "使用 OR 搜索 'rust async' 应该匹配 3 篇日记"
        );
        assert_eq!(
            unmatches.len(),
            1,
            "使用 OR 搜索 'rust async' 应该不匹配 1 篇日记"
        );

        // 清理测试数据
        let (ids, _) = page_diary_ids(&client, None)
            .await
            .expect("无法获取日记列表");
        for id in ids {
            let _ = delete_diary(&cache, &lfc, &client, &id).await;
        }
    }
}

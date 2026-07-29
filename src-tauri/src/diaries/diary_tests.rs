#[cfg(test)]
mod tests {
    use crate::caches::{DiaryMemoryCache, LocalObjectStore};
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::cryptos::Crypto;
    use crate::diaries::diary_migration::CURRENT_VERSION;
    use crate::diaries::diary_store::{DiaryStore, RemoteStore};
    use crate::diaries::diary_types::DiaryManifest;
    use crate::diaries::{delete_diary, get_diary, save_diary, update_diary_content_only};
    use crate::object::OssClient;
    use crate::storages::{remote_attachments_key, remote_manifest_key};
    use crate::stream::create_mock_stream;
    use crate::test_utils::TestOssGuard;
    use chrono::Utc;
    use serde_json::to_vec;
    use std::time::Duration;

    #[tokio::test]
    async fn test_diary_crud_lifecycle() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());

        // 测试创建
        let initial_content = "Integration test diary content.";
        let (summary, content) = save_diary(&cache, &crypto, &store, initial_content)
            .await
            .expect("未能保存日记");

        assert_eq!(content.searchable_text(), initial_content);
        assert!(!summary.id.is_empty());
        let id = summary.id.clone();

        // 测试读取 - 验证远端拉取并写入缓存
        let fetched_manifest = get_diary(&cache, &crypto, &store, &id)
            .await
            .expect("远程获取日记失败");

        assert_eq!(fetched_manifest.id, id);
        assert_eq!(fetched_manifest.content.searchable_text(), initial_content);

        // 为了确保 update 生成的时间戳严格大于前一次，休眠防 Flaky Test
        tokio::time::sleep(Duration::from_millis(5)).await;

        // 测试更新
        let updated_content = "Updated content for testing.";
        let updated_summary =
            update_diary_content_only(&cache, &crypto, &store, &id, updated_content)
                .await
                .expect("未能更新日记");

        assert!(updated_summary.updated > summary.updated);

        // 测试再次读取 - 验证缓存失效/更新机制
        let refetched_manifest = get_diary(&cache, &crypto, &store, &id)
            .await
            .expect("未能重新获取更新的日记");

        assert_eq!(
            refetched_manifest.content.searchable_text(),
            updated_content
        );

        // 删除必须覆盖同一日记前缀下的所有附件，并最后移除 manifest。
        for (attachment_id, data) in [
            ("att-one", b"one".as_slice()),
            ("att-two", b"two".as_slice()),
        ] {
            store
                .upload_attachment(
                    &id,
                    attachment_id,
                    data.len() as u64,
                    "application/octet-stream",
                    create_mock_stream(data.to_vec(), data.len()),
                )
                .await
                .expect("上传待删除附件失败");
        }

        // 测试删除
        delete_diary(&cache, &store, &id)
            .await
            .expect("删除日记失败");

        // 验证删除有效性
        let not_found_result = get_diary(&cache, &crypto, &store, &id).await;
        assert!(not_found_result.is_err(), "删除后日记不应被检索");
        let (remaining_objects, next_token) = client
            .list(&format!("{id}/"), None)
            .await
            .expect("检查删除后的日记对象失败");
        assert!(remaining_objects.is_empty(), "删除后不应残留日记附件");
        assert!(next_token.is_none());
        for key in [
            remote_manifest_key(&id),
            remote_attachments_key(&id, "att-one"),
            remote_attachments_key(&id, "att-two"),
        ] {
            assert!(
                los.get(&key).await.unwrap().is_none(),
                "本地缓存仍残留 {key}"
            );
        }
        _guard.cleanup().await;
    }

    #[tokio::test]
    async fn test_local_object_store_integration() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());

        // 保存第一篇日记
        let content1 = "Original content for cache test.";
        let (summary, _) = save_diary(&cache, &crypto, &store, content1)
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
        let cached = los.get(&object_key).await.expect("检查缓存失败");
        assert!(cached.is_some(), "本地缓存文件未生成");
        let cached_etag1 = cached.unwrap();
        assert_eq!(cached_etag1, etag1, "本地缓存的 etag 与 OSS 不一致");

        // 模拟外部直接修改 OSS 上的日记内容（绕过更新接口）
        // 构造一个新的 manifest，内容不同，id 相同
        let modified_manifest = DiaryManifest {
            id: id.clone(),
            algorithm: Gcm,
            content: "Modified content after external update.".into(),
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
        let fetched = get_diary(&cache2, &crypto, &store, &id)
            .await
            .expect("获取日记失败");
        assert_eq!(
            fetched.content, modified_manifest.content,
            "获取到的内容应为修改后的内容"
        );

        // 验证本地缓存已被更新为新 etag 和新内容
        let cached_after = los.get(&object_key).await.expect("检查缓存失败");
        assert!(cached_after.is_some(), "本地缓存应存在");
        let cached_etag2 = cached_after.unwrap();
        assert_eq!(cached_etag2, new_etag, "本地缓存的 etag 未更新");

        // 验证本地缓存文件解密后的内容是否正确
        let cached_bytes = los.get_data(&object_key).await.expect("读取缓存数据失败");
        let decrypted = crypto.decrypt(&cached_bytes).expect("解密缓存数据失败");
        let cached_manifest: DiaryManifest =
            serde_json::from_slice(&decrypted).expect("反序列化缓存数据失败");
        assert_eq!(
            cached_manifest.content, modified_manifest.content,
            "本地缓存文件内容与预期不符"
        );

        // 测试删除日记时本地缓存是否被清理
        delete_diary(&cache2, &store, &id)
            .await
            .expect("删除日记失败");
        let cached_after_delete = los.get(&object_key).await.expect("检查缓存失败");
        assert!(cached_after_delete.is_none(), "删除后本地缓存应被移除");
        _guard.cleanup().await;
    }
}

#[cfg(test)]
mod diary_list_tests {
    use crate::caches::{DiaryMemoryCache, LocalObjectStore};
    use crate::cryptos::Crypto;
    use crate::diaries::diary::save_diary;
    use crate::diaries::diary_store::RemoteStore;
    use crate::diaries::{get_diary_detail, get_diary_summary, page_diary_ids};
    use crate::object::OssClient;
    use crate::test_utils::TestOssGuard;

    #[tokio::test]
    async fn test_diary_list() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let (client, _guard) = TestOssGuard::new(client).await;
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = RemoteStore::new(los.clone(), client.clone());

        // 创建几个测试日记
        let title = "这是一个测试日记的标题";
        let content = "这是一个测试日记内容";
        let test_count = 21; // 测试环境下按10条分页，创建21条以测试分页逻辑
        for _ in 0..test_count {
            let _ = save_diary(
                &cache,
                &crypto,
                &store,
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
            let (ids, nt) = page_diary_ids(&store, next_token)
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
        assert!(
            all_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "远程日记列表应按反向时间戳 ID 升序排列，即最新日记在前"
        );
        for id in all_ids.clone() {
            let summary = get_diary_summary(&cache, &crypto, &store, &id)
                .await
                .expect("无法获取日记摘要");
            assert_eq!(summary.title, title);
            let detail = get_diary_detail(
                &cache,
                &crypto,
                &store,
                &crate::attachments::AttachmentServerHandle::for_test(),
                &id,
            )
            .await
            .expect("无法获取日记内容");
            assert_eq!(
                detail.content,
                format!("{}\n{}", title, content).as_str().into()
            );
            assert_eq!(detail.summary.id, id);
            assert!(detail.manifest_size > 0);
            assert!(detail.attachments.is_empty());
            assert!(detail.attachment_urls.is_empty());
        }
        _guard.cleanup().await;
    }
}

#[cfg(test)]
mod diary_search_tests {
    use crate::attachments::AttachmentMeta;
    use crate::caches::{DiaryMemoryCache, LocalObjectStore};
    use crate::cryptos::crypto_types::EncryptionAlgorithm::Gcm;
    use crate::cryptos::Crypto;
    use crate::diaries::diary::{save_diary, update_diary_attachment};
    use crate::diaries::diary_search::{search_diaries, SearchDiaryQuery};
    use crate::diaries::diary_store::{DiaryStore, LocalStore};
    use crate::diaries::diary_types::{AttachmentTypeFilter, DiarySummary, SearchDiariesEvent};
    use std::sync::Arc;

    async fn test_search(
        cache: &DiaryMemoryCache,
        crypto: &Crypto,
        store: &dyn DiaryStore,
        keyword: String,
        or: bool,
        attachment_types: Vec<AttachmentTypeFilter>,
        attachment_or: bool,
    ) -> (Vec<DiarySummary>, Vec<String>) {
        // 创建事件监听器
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SearchDiariesEvent>();
        let event_sender = Arc::new(tx);
        search_diaries(
            cache,
            crypto,
            store,
            event_sender.clone(),
            SearchDiaryQuery {
                keyword,
                keyword_or: or,
                attachment_types,
                attachment_or,
            },
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

    #[tokio::test]
    async fn test_diary_search() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let cache = DiaryMemoryCache::new();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().to_path_buf();
        let los = LocalObjectStore::new(path);
        let store = LocalStore::new(los);

        // 创建几个测试日记
        let (first, _) = save_diary(
            &cache,
            &crypto,
            &store,
            "这是第一篇日记，包含关键词 rust[[IMG:att-1]]",
        )
        .await
        .unwrap();
        let (second, _) = save_diary(
            &cache,
            &crypto,
            &store,
            "这是第二篇日记，不包含关键词[[AUD:att-1]][[IMG:att-2]]",
        )
        .await
        .unwrap();
        let (third, _) = save_diary(
            &cache,
            &crypto,
            &store,
            "这是第三篇日记，包含关键词 rust 和 async[[AUD:att-1]]",
        )
        .await
        .unwrap();
        let (fourth, _) = save_diary(
            &cache,
            &crypto,
            &store,
            "这是第四篇日记，包含关键词 async[[VID:att-1]]",
        )
        .await
        .unwrap();

        // 元数据中的 MIME 类型故意与正文节点冲突，附件筛选应以正文节点语义为准。
        for (id, mimetype) in [
            (&first.id, "audio/mpeg"),
            (&second.id, "video/mp4"),
            (&third.id, "image/jpeg"),
            (&fourth.id, "application/octet-stream"),
        ] {
            update_diary_attachment(
                &cache,
                &crypto,
                &store,
                id,
                AttachmentMeta {
                    id: "att-1".to_string(),
                    filename: "1".to_string(),
                    mimetype: mimetype.to_string(),
                    size: 1,
                    encrypted: false,
                    nonce: Vec::new(),
                    algorithm: Gcm,
                    etag: None,
                },
            )
            .await
            .unwrap();
        }
        update_diary_attachment(
            &cache,
            &crypto,
            &store,
            &second.id,
            AttachmentMeta {
                id: "att-2".to_string(),
                filename: "2".to_string(),
                mimetype: "text/plain".to_string(),
                size: 1,
                encrypted: false,
                nonce: Vec::new(),
                algorithm: Gcm,
                etag: None,
            },
        )
        .await
        .unwrap();

        // 收集结果
        let (matches, unmatches) = test_search(
            &cache,
            &crypto,
            &store,
            "rust".to_string(),
            true,
            vec![],
            true,
        )
        .await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'rust' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'rust' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) = test_search(
            &cache,
            &crypto,
            &store,
            "async".to_string(),
            true,
            vec![],
            true,
        )
        .await;
        assert_eq!(matches.len(), 2, "使用 OR 搜索 'async' 应该匹配 2 篇日记");
        assert_eq!(
            unmatches.len(),
            2,
            "使用 OR 搜索 'async' 应该不匹配 2 篇日记"
        );

        let (matches, unmatches) = test_search(
            &cache,
            &crypto,
            &store,
            "rust async".to_string(),
            false,
            vec![],
            true,
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
            &crypto,
            &store,
            "rust async".to_string(),
            true,
            vec![],
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
        let (matches, _) = test_search(
            &cache,
            &crypto,
            &store,
            String::new(),
            false,
            vec![AttachmentTypeFilter::Image],
            true,
        )
        .await;
        assert_eq!(matches.len(), 2, "只选择图片时应匹配 2 篇日记");
        assert!(matches.iter().any(|summary| summary.id == first.id));
        assert!(matches.iter().any(|summary| summary.id == second.id));

        let (matches, _) = test_search(
            &cache,
            &crypto,
            &store,
            String::new(),
            false,
            vec![AttachmentTypeFilter::Image, AttachmentTypeFilter::Audio],
            true,
        )
        .await;
        assert_eq!(matches.len(), 3, "多个附件类型之间应采用 OR 语义");

        let (matches, _) = test_search(
            &cache,
            &crypto,
            &store,
            String::new(),
            false,
            vec![AttachmentTypeFilter::Image, AttachmentTypeFilter::Audio],
            false,
        )
        .await;
        assert_eq!(matches.len(), 1, "AND 模式应要求日记包含每一种附件类型");
        assert_eq!(matches[0].id, second.id);

        let (matches, _) = test_search(
            &cache,
            &crypto,
            &store,
            "rust".to_string(),
            true,
            vec![AttachmentTypeFilter::Audio],
            true,
        )
        .await;
        assert_eq!(matches.len(), 1, "关键词和附件类型之间应采用 AND 语义");
        assert_eq!(matches[0].id, third.id);
    }
}

#[cfg(test)]
mod diary_migration_tests {
    use crate::caches::LocalObjectStore;
    use crate::diaries::diary_migration::{
        default_registry, get_version, legacy_attachment_id, migrate_manifest_bytes,
        MigrationContext, CURRENT_VERSION,
    };
    use crate::diaries::{DiaryError, LocalStore};
    use serde_json::Value;

    // ---- 基础工具函数 ----

    #[test]
    fn test_get_version_missing_field() {
        let json = serde_json::json!({"id": "test"});
        assert_eq!(get_version(&json), 1);
    }

    #[test]
    fn test_get_version_explicit() {
        let json = serde_json::json!({"id": "test", "version": 2});
        assert_eq!(get_version(&json), 2);
    }

    #[test]
    fn legacy_attachment_ids_are_deterministic_and_diary_scoped() {
        let first = legacy_attachment_id("diary-a", "photo.jpg");
        assert_eq!(first, legacy_attachment_id("diary-a", "photo.jpg"));
        assert_ne!(first, legacy_attachment_id("diary-a", "other.jpg"));
        assert_ne!(first, legacy_attachment_id("diary-b", "photo.jpg"));
        assert!(first.starts_with("att-"));
    }

    // ---- 边界情况 ----

    #[tokio::test]
    async fn test_no_migration_needed_when_already_current() {
        let tempdir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "test",
            store: &store,
        };
        let json_bytes = br#"{"id":"test","version":4,"content":{"nodes":[]},"attachments":[]}"#;
        assert!(migrate_manifest_bytes(&context, json_bytes)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_version_greater_than_current_requests_app_upgrade() {
        let mut json = serde_json::json!({"id": "test", "content": "hello"});
        json["version"] = Value::Number((CURRENT_VERSION + 1).into());
        let bytes = serde_json::to_vec(&json).unwrap();
        let tempdir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "test",
            store: &store,
        };
        let error = migrate_manifest_bytes(&context, &bytes).await.unwrap_err();

        assert!(matches!(
            error,
            DiaryError::UnsupportedVersion {
                found,
                supported: CURRENT_VERSION
            } if found == CURRENT_VERSION + 1
        ));
    }

    #[test]
    fn test_unsupported_version_maps_to_stable_app_error_type() {
        let error = DiaryError::UnsupportedVersion {
            found: CURRENT_VERSION + 1,
            supported: CURRENT_VERSION,
        };
        let app_error: crate::error::AppError = error.into();

        assert_eq!(app_error.error_type, "diary_version_too_new");
        assert!(app_error
            .message
            .contains(&(CURRENT_VERSION + 1).to_string()));
        assert!(app_error.message.contains(&CURRENT_VERSION.to_string()));
    }

    #[tokio::test]
    async fn test_v2_migration_groups_only_consecutive_images() {
        let source = serde_json::json!({
            "id": "migration-boundaries",
            "version": 2,
            "content": concat!(
                "prefix\n",
                "[[IMG:1.jpg]] \n [[IMG:2.jpg]]",
                "body",
                "[[IMG:3.jpg]]\n[[FILE:a.pdf]]\n[[IMG:4.jpg]]",
                "\n[[IMG:5.jpg]]\t[[IMG:6.jpg]]",
                "\nsuffix"
            ),
            "attachments": []
        });

        let bytes = serde_json::to_vec(&source).unwrap();
        let tempdir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "migration-boundaries",
            store: &store,
        };
        let new_bytes = migrate_manifest_bytes(&context, &bytes)
            .await
            .unwrap()
            .unwrap();
        let result: Value = serde_json::from_slice(&new_bytes).unwrap();
        let nodes = result["content"]["nodes"].as_array().unwrap();

        assert_eq!(get_version(&result), CURRENT_VERSION);
        assert_eq!(
            nodes.iter().filter(|node| node["type"] == "album").count(),
            2
        );
        assert_eq!(
            nodes[1]["attachmentIds"],
            serde_json::json!([
                legacy_attachment_id("migration-boundaries", "1.jpg"),
                legacy_attachment_id("migration-boundaries", "2.jpg")
            ])
        );
        assert_eq!(nodes[1]["id"], "migration-v3-album-1");
        assert!(nodes.iter().any(|node| node["type"] == "file"));
        assert!(nodes
            .iter()
            .any(|node| node["type"] == "image" && node.get("attachmentId").is_some()));
        assert_eq!(
            nodes
                .iter()
                .find(|node| node["id"] == "migration-v3-album-2")
                .unwrap()["attachmentIds"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[tokio::test]
    async fn test_v3_migration_rejects_duplicate_attachment_filenames() {
        let source = serde_json::json!({
            "id": "duplicate",
            "version": 3,
            "content": {"nodes": []},
            "attachments": [
                {"filename": "same.jpg"},
                {"filename": "same.jpg"}
            ]
        });
        let tempdir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "duplicate",
            store: &store,
        };
        let error = migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(error, DiaryError::InvalidManifest(message) if message.contains("duplicate"))
        );
    }

    #[tokio::test]
    async fn test_v3_migration_rejects_requested_diary_id_mismatch() {
        let source = serde_json::json!({
            "id": "manifest-id",
            "version": 3,
            "content": {"nodes": []},
            "attachments": []
        });
        let tempdir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "requested-id",
            store: &store,
        };
        let error = migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(error, DiaryError::InvalidManifest(message) if message.contains("does not match"))
        );
    }

    // ---- 各迁移步骤统一测试：test_input → 转换为最新版本 ----

    #[tokio::test]
    async fn test_each_step_input_migrates_to_latest() {
        let registry = default_registry();
        assert!(!registry.steps().is_empty(), "至少应有一个迁移步骤");

        for step in registry.steps() {
            let json = step.test_input();
            assert_eq!(
                get_version(&json),
                step.source_version(),
                "test_input 版本应为源版本 {}",
                step.source_version()
            );

            // 序列化后通过完整迁移管道升级到最新版本
            let bytes = serde_json::to_vec(&json).unwrap();
            let tempdir = tempfile::tempdir().unwrap();
            let store = LocalStore::new(LocalObjectStore::new(tempdir.path().to_path_buf()));
            let diary_id = json["id"].as_str().unwrap();
            let context = MigrationContext {
                diary_id,
                store: &store,
            };
            let new_bytes = migrate_manifest_bytes(&context, &bytes)
                .await
                .unwrap()
                .unwrap();

            let result: Value = serde_json::from_slice(&new_bytes).unwrap();
            assert_eq!(
                get_version(&result),
                CURRENT_VERSION,
                "V{} 迁移后应达到 CURRENT_VERSION",
                step.source_version()
            );

            // 验证迁移后的数据符合该步骤的预期
            step.test_verify(&result);
        }
    }
}

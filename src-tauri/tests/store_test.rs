#[cfg(test)]
mod secure_store {
    use chrono::Utc;
    use std::sync::Arc;
    use surkaa_pad_lib::encryption_manager::EncryptionManager;
    use surkaa_pad_lib::oss_client_manager::OssClientManager;
    use surkaa_pad_lib::secure_diary_store::SecureDiaryStore;
    use tokio;

    const KEY: &str = "";
    const SECRET: &str = "";
    const ENDPOINT: &str = "";
    const BUCKET_NAME: &str = "";

    async fn create_store() -> SecureDiaryStore {
        // 初始化 OSS 客户端管理器
        let oss = OssClientManager::default();
        oss.initialize(KEY, SECRET, ENDPOINT, BUCKET_NAME)
            .await
            .expect("Failed to initialize OSS client");

        let client = Arc::new(oss);

        let mut encryption = EncryptionManager::new();

        encryption
            .initial("strong_password", "dGVzdF9zYWx0")
            .expect("Failed to initialize encryption manager");

        SecureDiaryStore::new(client, encryption)
    }

    #[tokio::test]
    async fn test_list_diaries() {
        let store = create_store().await;

        let diary_ids = store.list_diaries().await.expect("Failed to list diaries");

        println!("Diary IDs: {:?}", diary_ids);
    }

    #[tokio::test]
    async fn test_create_and_get_diary() {
        let store = create_store().await;

        let content = "This is a test diary content.";

        let new_id = store
            .create_diary(content)
            .await
            .expect("Failed to create diary");

        println!("Created new diary with ID: {}", new_id);

        // 验证新创建的日记是否在列表中
        let diary_ids = store
            .list_diaries()
            .await
            .expect("Failed to list diaries")
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<String>>();
        assert!(
            diary_ids.contains(&new_id),
            "New diary ID not found in the list: {}",
            diary_ids.join(", ")
        );

        // 获取刚创建的日记内容
        let (diary, _) = store
            .get_diary_manifest(new_id.clone())
            .await
            .expect("Failed to get diary manifest");

        assert_eq!(diary.id, new_id, "Diary ID does not match");
        assert_eq!(
            diary.algorithm, "AES256-GCM_v1",
            "Diary algorithm does not match"
        );
        assert_eq!(diary.content, content, "Diary content does not match");
        // 判断创建时间和更新时间是否在当前时间的一分钟内
        let now = Utc::now().timestamp();
        assert!(
            (now - diary.created_at).abs() < 60,
            "Diary created_at timestamp is not recent"
        );
        assert!(
            (now - diary.updated_at).abs() < 60,
            "Diary updated_at timestamp is not recent"
        );
        assert!(
            diary.attachments.is_empty(),
            "Diary attachments should be empty"
        );

        println!("Diary manifest verified successfully.");
    }

    #[tokio::test]
    async fn test_delete_diary() {
        let store = create_store().await;

        let content = "This is a test diary content to be deleted.";

        let new_id = store
            .create_diary(content)
            .await
            .expect("Failed to create diary");

        println!("Created new diary with ID: {}", new_id);

        // 删除日记
        store
            .delete_diary(new_id.clone())
            .await
            .expect("Failed to delete diary");

        // 验证日记已被删除
        let result = store.get_diary_manifest(new_id.clone()).await;
        assert!(
            result.is_err(),
            "Expected error when fetching deleted diary, but got success"
        );

        println!("Diary with ID: {} deleted successfully.", new_id);
    }

    #[tokio::test]
    async fn test_update_diary_content() {
        let store = create_store().await;

        let content = "This is the original diary content.";

        let new_id = store
            .create_diary(content)
            .await
            .expect("Failed to create diary");

        println!("Created new diary with ID: {}", new_id);

        // 更新日记内容
        let updated_content = "This is the updated diary content.";
        store
            .update_diary_content_only(new_id.clone(), updated_content)
            .await
            .expect("Failed to update diary content");

        // 获取更新后的日记内容
        let (diary, _) = store
            .get_diary_manifest(new_id.clone())
            .await
            .expect("Failed to get diary manifest");

        assert_eq!(
            diary.content, updated_content,
            "Diary content was not updated correctly"
        );

        println!("Diary content updated and verified successfully.");
    }

    #[tokio::test]
    async fn test_update_diary_attachments() {
        let store = create_store().await;

        let content = "This is the original diary content.";

        let new_id = store
            .create_diary(content)
            .await
            .expect("Failed to create diary");

        println!("Created new diary with ID: {}", new_id);

        // 更新日记附件
        let attachment_bytes = b"Sample attachment data".to_vec();

        // 添加附件
        store
            .add_attachment(new_id.clone(), attachment_bytes.clone(), "png".to_string())
            .await
            .expect("Failed to add attachment");

        print!("Added attachment to diary ID: {}", new_id);

        // 获取更新后的日记内容
        let (diary, _) = store
            .get_diary_manifest(new_id.clone())
            .await
            .expect("Failed to get diary manifest");

        assert_eq!(
            diary.attachments.len(),
            1,
            "Attachment was not added correctly"
        );
        let attachment = &diary.attachments[0];
        assert_eq!(
            attachment.mime_type, "png",
            "Attachment file extension mismatch"
        );

        // 解密并验证附件内容
        let downloaded_attachment = store
            .download_attachment(
                new_id.clone(),
                attachment.file_name.clone(),
                attachment.nonce.clone(),
            )
            .await
            .expect("Failed to download attachment");

        assert_eq!(
            downloaded_attachment, attachment_bytes,
            "Downloaded attachment content mismatch"
        );
        println!("Diary attachment added and verified successfully.");
    }

    #[tokio::test]
    async fn test_delete_attachment() {
        let store = create_store().await;

        let content = "This is the original diary content.";

        let new_id = store
            .create_diary(content)
            .await
            .expect("Failed to create diary");

        println!("Created new diary with ID: {}", new_id);

        // 添加附件
        let attachment_bytes = b"Sample attachment data".to_vec();
        store
            .add_attachment(new_id.clone(), attachment_bytes, "txt".to_string())
            .await
            .expect("Failed to add attachment");

        // 获取日记以获取附件信息
        let (diary, _) = store
            .get_diary_manifest(new_id.clone())
            .await
            .expect("Failed to get diary manifest");
        assert_eq!(
            diary.attachments.len(),
            1,
            "Attachment was not added correctly"
        );
        let attachment = &diary.attachments[0];

        // 删除附件
        store
            .delete_attachment(new_id.clone(), attachment.file_name.clone())
            .await
            .expect("Failed to delete attachment");

        // 验证附件已被删除
        let (updated_diary, _) = store
            .get_diary_manifest(new_id.clone())
            .await
            .expect("Failed to get updated diary manifest");

        assert!(
            updated_diary.attachments.is_empty(),
            "Attachment was not deleted correctly"
        );

        println!("Attachment deleted successfully from diary ID: {}", new_id);
    }
}

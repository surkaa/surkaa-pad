use crate::attachments::types::AddAttachmentEvent;
use crate::attachments::AttachmentMeta;
use crate::crypto::types::EncryptionAlgorithm::Ctr;
use crate::crypto::Crypto;
use crate::diaries::{delete_diary_attachment, get_diary, update_diary_attachment, DiaryMemoryCache};
use crate::object::tracker_stream::tracker_stream;
use crate::object::{ByteStream, OssClient};
use crate::storages::remote_attachments_key;
use crate::utils::message_sender::MessageSender;
use dashmap::DashMap;
use std::collections::HashSet;
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
    event: Arc<dyn MessageSender<AddAttachmentEvent>>,
    id: &str,
    encrypted: bool,
    size: u64,
    mimetype: String,
    stream: ByteStream,
) {
    let _ = event.send(AddAttachmentEvent::Started);
    // 包装流 用来更新进度
    let ec = event.clone();
    let stream = tracker_stream(size, stream, move |progress| {
        let _ = ec.send(AddAttachmentEvent::Progress(progress));
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
        Ok::<AttachmentMeta, String>(attachment)
    };

    match logic.await {
        Err(e) => {
            let _ = event.send(AddAttachmentEvent::Error(e));
        }
        Ok(attachment) => {
            let _ = event.send(AddAttachmentEvent::Completed(attachment));
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::diaries::{delete_diary, page_diary_ids, save_diary};
    use crate::object::create_mock_stream;
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
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<AddAttachmentEvent>();

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
}

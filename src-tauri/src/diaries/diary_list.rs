use crate::crypto::Crypto;
use crate::diaries::types::DiarySummary;
use crate::diaries::{get_diary, DiaryMemoryCache};
use crate::object::{NextToken, OssClient};
use crate::storages::{diary_id_from_manifest_key, remote_attachments_key};
use std::collections::HashMap;

pub async fn page_diary_ids(
    client: &OssClient,
    next_token: NextToken,
) -> Result<(Vec<String>, NextToken), String> {
    let (objects, nt) = client
        .list("", next_token)
        .await
        .map_err(|e| format!("获取列表失败:{}", e))?;
    let mut ids: Vec<String> = Vec::with_capacity(objects.len());
    for obj in objects {
        if let Some(id) = diary_id_from_manifest_key(obj.key()) {
            ids.push(id);
        }
    }
    Ok((ids, nt))
}

pub async fn get_diary_summary(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<DiarySummary, String> {
    let diary = get_diary(cache, crypto, client, id).await?;
    Ok(DiarySummary::from_manifest(diary))
}

pub async fn get_diary_content(
    cache: &DiaryMemoryCache,
    crypto: &Crypto,
    client: &OssClient,
    id: &str,
) -> Result<(String, HashMap<String, String>), String> {
    const ATTACHMENT_URL_EXPIRATION_SECONDS: u64 = 3600; // 附件URL过期时间，单位秒
    let diary = get_diary(cache, crypto, client, id).await?;
    let mut map = HashMap::new();
    for attachment in diary.attachments {
        if attachment.encrypted {
            continue;
        }
        let key = remote_attachments_key(id, &attachment.filename);
        let url = client
            .direct_url(&key, ATTACHMENT_URL_EXPIRATION_SECONDS)
            .map_err(|e| format!("生成附件URL失败:{}", e))?;
        map.insert(attachment.filename, url);
    }
    Ok((diary.content, map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diaries::diary::{delete_diary, save_diary};
    use serial_test::serial;

    #[serial]
    #[tokio::test]
    async fn test_diary_list() {
        // 初始化依赖
        let crypto = Crypto::from_env();
        let client = OssClient::from_env();
        let cache = DiaryMemoryCache::new();

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
            let _ = save_diary(&crypto, &client, format!("{}\n{}", title, content).as_str())
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
            let summary = get_diary_summary(&cache, &crypto, &client, &id)
                .await
                .expect("无法获取日记摘要");
            assert_eq!(summary.title, title);
            let content = get_diary_content(&cache, &crypto, &client, &id)
                .await
                .expect("无法获取日记内容");
            assert_eq!(content, content);
        }

        // 清理测试数据
        for id in all_ids {
            let _ = delete_diary(&cache, &client, &id)
                .await
                .expect("无法删除测试日记");
        }
    }
}

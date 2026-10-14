//! OSS 的版本控制桶不支持原子的“禁止覆盖”。每个任务先发布独立的占位对象，
//! 再通过强一致的完整枚举确认自己是唯一任务，成功者保留占位直到最终确认结束。
//! 同时竞争可能让两方都失败，但绝不能让两方同时修改数据；不按时间抢占旧任务。
use super::storage::{byte_stream, read_limited, RekeyStorage};
use super::{error, Result};
use crate::object_locations::ObjectLocations;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CloudClaim {
    version: u32,
    transaction_id: String,
    vault_id: String,
}

impl CloudClaim {
    pub fn new(transaction_id: &str, vault_id: &str) -> Result<Self> {
        if [transaction_id, vault_id].iter().any(|id| {
            id.len() != 32
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(error("改密占位记录的身份无效"));
        }
        Ok(Self {
            version: 1,
            transaction_id: transaction_id.into(),
            vault_id: vault_id.into(),
        })
    }

    fn key(&self) -> String {
        ObjectLocations::vault_key_change_claim(&self.transaction_id)
    }

    async fn verify(&self, store: &dyn RekeyStorage) -> Result<()> {
        let bytes = read_limited(store, &self.key(), 4096).await?;
        let actual: Self = serde_json::from_slice(&bytes).map_err(error)?;
        if actual != *self {
            return Err(error("改密占位记录与本机任务不匹配，不能覆盖或删除"));
        }
        Ok(())
    }

    /// 调用前必须已经持久保存本机 Journal。PUT 响应丢失也保留记录供续改核对。
    pub async fn acquire(&self, store: &dyn RekeyStorage) -> Result<bool> {
        let key = self.key();
        if store.head(&key).await?.is_none() {
            let bytes = serde_json::to_vec(self).map_err(error)?;
            store
                .write(&key, bytes.len() as u64, byte_stream(bytes))
                .await?;
        }
        self.verify(store).await?;
        let claims = store
            .list_prefix(ObjectLocations::vault_key_change_claims_prefix())
            .await?;
        if !claims.iter().any(|claim| claim.key == key) {
            return Err(error("无法确认云端改密占位，不能开始修改数据"));
        }
        // 不选“最小 ID”赢家：已开始运行的任务可能比后来者的 ID 大。
        // 已获准的任务不会释放占位，后来者必定看到冲突并停止。
        Ok(claims.iter().all(|claim| claim.key == key))
    }

    /// 只移除当前对象，不枚举或永久删除历史版本。调用方必须先清理活动 Journal。
    pub async fn release(&self, store: &dyn RekeyStorage) -> Result<()> {
        if store.head(&self.key()).await?.is_none() {
            return Ok(());
        }
        self.verify(store).await?;
        store.delete(&self.key()).await
    }
}

#[cfg(test)]
#[path = "cloud_claim_tests.rs"]
mod tests;

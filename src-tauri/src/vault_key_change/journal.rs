use super::types::{Checkpoint, Journal, LocalEncryptedConfig, WorkItem};
use super::{error, Result};
use crate::cryptos::Crypto;
use crate::vault_bootstrap::VaultBootstrap;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub(super) const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
pub(super) const CURRENT_KEY_CHANGE_VERSION: u32 = 2;

impl Journal {
    fn context(&self) -> Result<Vec<u8>> {
        Ok(Sha256::digest(
            serde_json::to_vec(&(
                "surkaa-pad:key-change:v1",
                self.version,
                &self.transaction_id,
                self.remote,
                &self.old_bootstrap,
                &self.new_bootstrap,
            ))
            .map_err(error)?,
        )
        .to_vec())
    }

    pub fn create(
        old: &Crypto,
        new: &Crypto,
        old_bootstrap: VaultBootstrap,
        new_bootstrap: VaultBootstrap,
        remote: bool,
        items: Vec<WorkItem>,
        local_config: LocalEncryptedConfig,
    ) -> Result<(Self, Checkpoint)> {
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(error)?;
        let mut journal = Self {
            version: CURRENT_KEY_CHANGE_VERSION,
            transaction_id: hex::encode(random),
            remote,
            old_bootstrap,
            new_bootstrap,
            old_key_under_new: vec![],
            encrypted_checkpoint: vec![],
        };
        let context = journal.context()?;
        journal.old_key_under_new = new.wrap_peer(old, &context).map_err(error)?;
        let checkpoint = Checkpoint {
            header_hash: hex::encode(context),
            items,
            next: 0,
            staged: None,
            new_etags: Default::default(),
            local_config,
            data_committed: false,
            refresh_cloud_cache: false,
        };
        journal.update(new, &checkpoint)?;
        Ok((journal, checkpoint))
    }

    pub fn update(&mut self, new: &Crypto, checkpoint: &Checkpoint) -> Result<()> {
        let bytes = Zeroizing::new(serde_json::to_vec(checkpoint).map_err(error)?);
        self.encrypted_checkpoint = new.encrypt(&bytes).map_err(error)?;
        // 在任何原对象写入之前发现过大的恢复记录，不留无法续改的半成品。
        self.encode()?;
        Ok(())
    }

    /// V1 -> V2 新增临时对象 ETag 与源 CRC。没有证据的旧检查点仍完整读回恢复。
    /// 版本参与密钥封装的认证上下文，升级必须同时重新封装旧密钥及加密检查点，
    /// 不能只改 JSON 中的 version。调用方在新记录持久成功后才采用升级后的副本。
    pub fn migrate(
        &mut self,
        old: &Crypto,
        new: &Crypto,
        checkpoint: &mut Checkpoint,
    ) -> Result<bool> {
        match self.version {
            1 => {
                self.version = 2;
                let context = self.context()?;
                self.old_key_under_new = new.wrap_peer(old, &context).map_err(error)?;
                checkpoint.header_hash = hex::encode(context);
                self.update(new, checkpoint)?;
                Ok(true)
            }
            CURRENT_KEY_CHANGE_VERSION => Ok(false),
            _ => Err(error("不支持此版本的改密记录")),
        }
    }

    pub fn encode(&self) -> Result<String> {
        let json = serde_json::to_string(self).map_err(error)?;
        if json.len() > MAX_JOURNAL_BYTES {
            return Err(error("改密记录过大，尚未修改数据"));
        }
        Ok(json)
    }

    pub fn parse(json: &str) -> Result<Self> {
        if json.len() > MAX_JOURNAL_BYTES {
            return Err(error("改密记录超过大小限制"));
        }
        let journal: Self = serde_json::from_str(json).map_err(error)?;
        if !(1..=CURRENT_KEY_CHANGE_VERSION).contains(&journal.version)
            || journal.transaction_id.len() != 32
            || !journal
                .transaction_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || journal.old_bootstrap.vault_id != journal.new_bootstrap.vault_id
            || journal.old_bootstrap.kdf.salt == journal.new_bootstrap.kdf.salt
        {
            return Err(error("改密记录格式或 Vault 身份无效"));
        }
        journal.old_bootstrap.validate().map_err(error)?;
        journal.new_bootstrap.validate().map_err(error)?;
        Ok(journal)
    }

    pub fn open(&self, password: String) -> Result<(Crypto, Crypto, Checkpoint)> {
        let password = Zeroizing::new(password);
        let candidate = Crypto::new();
        let context = self.context()?;
        candidate
            .derive_and_verify_bootstrap(password.to_string(), &self.new_bootstrap)
            .map_err(|_| error("密码不正确，请输入改密后的主密码；仅更换盐时仍使用原密码"))?;
        let old = candidate
            .unwrap_peer(
                &self.old_key_under_new,
                &context,
                self.old_bootstrap.kdf.clone(),
            )
            .map_err(error)?;
        let new = candidate;
        let checkpoint = self.checkpoint(&new)?;
        Ok((old, new, checkpoint))
    }

    pub fn checkpoint(&self, new: &Crypto) -> Result<Checkpoint> {
        let context = self.context()?;
        let bytes = Zeroizing::new(new.decrypt(&self.encrypted_checkpoint).map_err(error)?);
        let checkpoint: Checkpoint = serde_json::from_slice(&bytes).map_err(error)?;
        if checkpoint.header_hash != hex::encode(context)
            || checkpoint.next > checkpoint.items.len()
            || (checkpoint.next == checkpoint.items.len() && checkpoint.staged.is_some())
        {
            return Err(error("改密检查点校验失败"));
        }
        // 恢复记录中 Key 必须能被本应用识别，避免受损记录导致任意对象被覆盖。
        for item in &checkpoint.items {
            if crate::object_locations::ObjectLocations::parse(&item.source.key).is_none() {
                return Err(error("改密记录包含不合法的对象位置"));
            }
        }
        Ok(checkpoint)
    }
}

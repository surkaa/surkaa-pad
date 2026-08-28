use super::{
    KeyDerivationParameters, VaultBootstrap, VaultBootstrapError, VAULT_BOOTSTRAP_SCHEMA_VERSION,
    VAULT_VERIFIER_TEXT,
};
use crate::app_config::AppConfigStore;
use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::object_locations::ObjectLocations;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const MAX_BOOTSTRAP_BYTES: u64 = 16 * 1024;

#[derive(Clone)]
pub struct VaultBootstrapRepository {
    app_config: AppConfigStore,
    oss_client: OssClient,
    crypto: Crypto,
}

struct RemoteInventory {
    encrypted_probe: Option<Vec<u8>>,
    has_objects: bool,
}

impl VaultBootstrapRepository {
    pub fn new(app_config: AppConfigStore, oss_client: OssClient, crypto: Crypto) -> Self {
        Self {
            app_config,
            oss_client,
            crypto,
        }
    }

    pub fn get_local(&self) -> Option<VaultBootstrap> {
        self.app_config.current().vault_bootstrap().cloned()
    }

    pub fn get_required(&self) -> Result<VaultBootstrap, VaultBootstrapError> {
        self.get_local().ok_or(VaultBootstrapError::NotInitialized)
    }

    pub fn export_json(&self) -> Result<String, VaultBootstrapError> {
        self.get_required()?.to_pretty_json()
    }

    /// 在旧版 Vault 已经由原有校验链路确认密码正确后，补建完整的引导配置。
    pub fn commit_active(&self) -> Result<VaultBootstrap, VaultBootstrapError> {
        if let Some(existing) = self.get_local() {
            self.verify_with_active_key(&existing)?;
            return Ok(existing);
        }

        let bootstrap = self.create_from_active_key()?;
        self.persist_local(bootstrap.clone())?;
        Ok(bootstrap)
    }

    pub async fn prepare_remote(
        &self,
        master_password: String,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        if let Some(remote) = self.load_remote().await? {
            if self
                .get_local()
                .is_some_and(|local| !local.same_vault_definition(&remote))
            {
                return Err(VaultBootstrapError::VaultMismatch);
            }
            self.crypto
                .derive_and_verify_bootstrap(master_password, &remote)?;
            self.persist_local(remote.clone())?;
            return Ok(remote);
        }

        let inventory = self.inspect_remote_inventory().await?;
        if let Some(local) = self.get_local() {
            self.crypto
                .derive_and_verify_bootstrap(master_password, &local)?;
            self.verify_remote_probe(&inventory)?;
            return self.create_remote_if_absent(local).await;
        }

        if let Some(probe) = inventory.encrypted_probe.as_deref() {
            let parameters = KeyDerivationParameters::legacy_current();
            self.crypto
                .derive_and_verify_ciphertext(master_password, parameters, probe)
                .map_err(|_| VaultBootstrapError::VerifierMismatch)?;
        } else if inventory.has_objects {
            return Err(VaultBootstrapError::Storage(
                "云端包含对象，但没有可用于确认旧版密钥的日记、设置或 AI 会话".into(),
            ));
        } else {
            self.crypto
                .derive_dek_with_parameters(
                    master_password,
                    KeyDerivationParameters::legacy_current(),
                )
                .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        }

        let bootstrap = self.commit_active()?;
        self.create_remote_if_absent(bootstrap).await
    }

    pub async fn ensure_remote_for_active_key(
        &self,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        let local = self.commit_active()?;
        if let Some(remote) = self.load_remote().await? {
            self.verify_with_active_key(&remote)?;
            if !local.same_vault_definition(&remote) {
                return Err(VaultBootstrapError::VaultMismatch);
            }
            return Ok(remote);
        }
        let inventory = self.inspect_remote_inventory().await?;
        self.verify_remote_probe(&inventory)?;
        self.create_remote_if_absent(local).await
    }

    pub async fn import_json(
        &self,
        json: &str,
        master_password: String,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        let imported = VaultBootstrap::from_json(json)?;
        if !self
            .crypto
            .validate_bootstrap_for_active_key(master_password, &imported)?
        {
            return Err(VaultBootstrapError::VaultMismatch);
        }
        if self
            .get_local()
            .is_some_and(|local| !local.same_vault_definition(&imported))
        {
            return Err(VaultBootstrapError::VaultMismatch);
        }

        if self.oss_client.is_initialized() {
            if let Some(remote) = self.load_remote().await? {
                self.verify_with_active_key(&remote)?;
                if !remote.same_vault_definition(&imported) {
                    return Err(VaultBootstrapError::VaultMismatch);
                }
            } else {
                self.create_remote_if_absent(imported.clone()).await?;
            }
        }
        self.persist_local(imported.clone())?;
        Ok(imported)
    }

    fn create_from_active_key(&self) -> Result<VaultBootstrap, VaultBootstrapError> {
        let mut vault_id = [0_u8; 16];
        getrandom::fill(&mut vault_id)
            .map_err(|error| VaultBootstrapError::Storage(error.to_string()))?;
        let encrypted_verifier = self
            .crypto
            .encrypt(VAULT_VERIFIER_TEXT.as_bytes())
            .map_err(|error| VaultBootstrapError::Storage(error.to_string()))?;
        let bootstrap = VaultBootstrap {
            schema_version: VAULT_BOOTSTRAP_SCHEMA_VERSION,
            vault_id: hex::encode(vault_id),
            kdf: self
                .crypto
                .active_kdf_parameters()
                .map_err(|error| VaultBootstrapError::Storage(error.to_string()))?,
            encrypted_verifier: STANDARD.encode(encrypted_verifier),
        };
        bootstrap.validate()?;
        Ok(bootstrap)
    }

    fn verify_with_active_key(
        &self,
        bootstrap: &VaultBootstrap,
    ) -> Result<(), VaultBootstrapError> {
        bootstrap.validate()?;
        let encrypted = bootstrap.decode_verifier()?;
        let plaintext = self
            .crypto
            .decrypt(&encrypted)
            .map_err(|_| VaultBootstrapError::VerifierMismatch)?;
        if plaintext == VAULT_VERIFIER_TEXT.as_bytes() {
            Ok(())
        } else {
            Err(VaultBootstrapError::VerifierMismatch)
        }
    }

    fn verify_remote_probe(&self, inventory: &RemoteInventory) -> Result<(), VaultBootstrapError> {
        if let Some(probe) = inventory.encrypted_probe.as_deref() {
            self.crypto
                .decrypt(probe)
                .map_err(|_| VaultBootstrapError::VaultMismatch)?;
            Ok(())
        } else if inventory.has_objects {
            Err(VaultBootstrapError::Storage(
                "云端包含对象，但无法确认它们是否属于当前 Vault".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn persist_local(&self, bootstrap: VaultBootstrap) -> Result<(), VaultBootstrapError> {
        self.app_config
            .set_vault_bootstrap(bootstrap)
            .map_err(|error| VaultBootstrapError::Storage(error.to_string()))
    }

    async fn load_remote(&self) -> Result<Option<VaultBootstrap>, VaultBootstrapError> {
        let key = ObjectLocations::vault_bootstrap();
        if !self
            .oss_client
            .object_exists(key)
            .await
            .map_err(storage_error)?
        {
            return Ok(None);
        }
        let metadata = self
            .oss_client
            .get_metadata(key)
            .await
            .map_err(storage_error)?;
        if metadata
            .content_length
            .is_some_and(|length| length > MAX_BOOTSTRAP_BYTES)
        {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "云端引导配置超过 {} 字节限制",
                MAX_BOOTSTRAP_BYTES
            )));
        }
        let bytes = self
            .oss_client
            .download_bytes(key)
            .await
            .map_err(storage_error)?;
        if bytes.len() as u64 > MAX_BOOTSTRAP_BYTES {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "云端引导配置超过 {} 字节限制",
                MAX_BOOTSTRAP_BYTES
            )));
        }
        let bootstrap = serde_json::from_slice::<VaultBootstrap>(&bytes)?;
        bootstrap.validate()?;
        Ok(Some(bootstrap))
    }

    async fn create_remote_if_absent(
        &self,
        bootstrap: VaultBootstrap,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        let data = serde_json::to_vec_pretty(&bootstrap)?;
        if self
            .oss_client
            .upload_bytes_if_absent(ObjectLocations::vault_bootstrap(), &data)
            .await
            .map_err(storage_error)?
        {
            return Ok(bootstrap);
        }

        let existing = self.load_remote().await?.ok_or_else(|| {
            VaultBootstrapError::Storage("云端引导配置创建冲突后仍无法读取".into())
        })?;
        self.verify_with_active_key(&existing)?;
        if existing.kdf != bootstrap.kdf {
            return Err(VaultBootstrapError::VaultMismatch);
        }
        // 两个设备同时迁移旧 Vault 时可能各自生成 Vault ID；以条件写入的胜者为准。
        self.persist_local(existing.clone())?;
        Ok(existing)
    }

    async fn inspect_remote_inventory(&self) -> Result<RemoteInventory, VaultBootstrapError> {
        if self
            .oss_client
            .object_exists(ObjectLocations::synced_settings())
            .await
            .map_err(storage_error)?
        {
            return Ok(RemoteInventory {
                encrypted_probe: Some(
                    self.oss_client
                        .download_bytes(ObjectLocations::synced_settings())
                        .await
                        .map_err(storage_error)?,
                ),
                has_objects: true,
            });
        }

        let (session_prefixes, _) = self
            .oss_client
            .list_common_prefixes(ObjectLocations::ai_sessions_prefix(), None)
            .await
            .map_err(storage_error)?;
        for prefix in session_prefixes {
            let Some(session_id) = ObjectLocations::ai_session_id_from_common_prefix(&prefix)
            else {
                continue;
            };
            let key = ObjectLocations::ai_session_meta(&session_id);
            if self
                .oss_client
                .object_exists(&key)
                .await
                .map_err(storage_error)?
            {
                return Ok(RemoteInventory {
                    encrypted_probe: Some(
                        self.oss_client
                            .download_bytes(&key)
                            .await
                            .map_err(storage_error)?,
                    ),
                    has_objects: true,
                });
            }
        }

        let (diary_prefixes, _) = self
            .oss_client
            .list_common_prefixes(ObjectLocations::diaries_prefix(), None)
            .await
            .map_err(storage_error)?;
        for prefix in diary_prefixes {
            let Some(diary_id) = ObjectLocations::diary_id_from_common_prefix(&prefix) else {
                continue;
            };
            let key = ObjectLocations::diary_manifest(&diary_id);
            if self
                .oss_client
                .object_exists(&key)
                .await
                .map_err(storage_error)?
            {
                return Ok(RemoteInventory {
                    encrypted_probe: Some(
                        self.oss_client
                            .download_bytes(&key)
                            .await
                            .map_err(storage_error)?,
                    ),
                    has_objects: true,
                });
            }
        }

        let (objects, next_token) = self
            .oss_client
            .list("", None)
            .await
            .map_err(storage_error)?;
        Ok(RemoteInventory {
            encrypted_probe: None,
            has_objects: !objects.is_empty() || next_token.is_some(),
        })
    }
}

fn storage_error(error: impl std::fmt::Display) -> VaultBootstrapError {
    VaultBootstrapError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;
    use crate::test_utils::TestOssGuard;

    fn repository() -> VaultBootstrapRepository {
        let crypto = Crypto::new();
        crypto
            .derive_dek_with_parameters("password".into(), KeyDerivationParameters::legacy_debug())
            .unwrap();
        VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            OssClient::new(),
            crypto,
        )
    }

    #[test]
    fn commit_creates_stable_local_bootstrap() {
        let repository = repository();
        let first = repository.commit_active().unwrap();
        let second = repository.commit_active().unwrap();

        assert_eq!(first, second);
        assert_eq!(first.kdf, KeyDerivationParameters::legacy_debug());
        assert_eq!(first.vault_id.len(), 32);
    }

    #[tokio::test]
    async fn import_requires_the_current_vault_key() {
        let repository = repository();
        let bootstrap = repository.commit_active().unwrap();
        let json = bootstrap.to_pretty_json().unwrap();

        assert_eq!(
            repository
                .import_json(&json, "password".into())
                .await
                .unwrap(),
            bootstrap
        );
        assert!(repository
            .import_json(&json, "wrong password".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn cloud_bootstrap_is_adopted_before_deriving_the_new_device_key() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let first_crypto = Crypto::new();
        first_crypto
            .derive_dek_with_parameters(
                "shared password".into(),
                KeyDerivationParameters::legacy_debug(),
            )
            .unwrap();
        let first = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            client.clone(),
            first_crypto,
        );
        let expected = first.commit_active().unwrap();
        assert_eq!(
            first.ensure_remote_for_active_key().await.unwrap(),
            expected
        );
        assert!(!client
            .upload_bytes_if_absent(ObjectLocations::vault_bootstrap(), b"must not overwrite")
            .await
            .unwrap());
        let persisted = client
            .download_bytes(ObjectLocations::vault_bootstrap())
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<VaultBootstrap>(&persisted).unwrap(),
            expected
        );

        let second = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            client,
            Crypto::new(),
        );
        let adopted = second
            .prepare_remote("shared password".into())
            .await
            .unwrap();
        assert_eq!(adopted, expected);
        assert_eq!(second.get_local(), Some(expected));

        guard.cleanup().await;
    }
}

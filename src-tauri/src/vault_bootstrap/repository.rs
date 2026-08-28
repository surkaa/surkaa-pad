use super::{
    KeyDerivationParameters, VaultBootstrap, VaultBootstrapError, VAULT_BOOTSTRAP_SCHEMA_VERSION,
    VAULT_VERIFIER_TEXT,
};
use crate::app_config::AppConfigStore;
use crate::caches::LocalObjectStore;
use crate::cryptos::Crypto;
use crate::object::OssClient;
use crate::object_locations::{ObjectLocations, StoredObject};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const MAX_BOOTSTRAP_BYTES: u64 = 16 * 1024;

#[derive(Clone)]
pub struct VaultBootstrapRepository {
    app_config: AppConfigStore,
    oss_client: OssClient,
    crypto: Crypto,
}

struct VaultInventory {
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

    /// 为确认没有历史数据的新 Vault 生成随机盐，并立即持久化可验证的引导配置。
    pub fn initialize_new(
        &self,
        master_password: String,
        memory_cost_kib: u32,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        if self.get_local().is_some() {
            return Err(VaultBootstrapError::AlreadyInitialized);
        }
        let parameters = KeyDerivationParameters::new_random(memory_cost_kib)?;
        self.crypto
            .derive_dek_with_parameters(master_password, parameters)
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        self.commit_active()
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
        new_vault_memory_cost_kib: u32,
        may_create_new_vault: bool,
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
            if !may_create_new_vault {
                return Err(VaultBootstrapError::ExistingLocalData);
            }
            self.crypto
                .derive_dek_with_parameters(
                    master_password.clone(),
                    KeyDerivationParameters::new_random(new_vault_memory_cost_kib)?,
                )
                .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
            let bootstrap = self.commit_active()?;
            return self
                .create_remote_for_new_vault(bootstrap, master_password)
                .await;
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
        local_object_store: &LocalObjectStore,
    ) -> Result<VaultBootstrap, VaultBootstrapError> {
        let imported = VaultBootstrap::from_json(json)?;
        if let Some(local) = self.get_local() {
            if !local.same_vault_definition(&imported) {
                return Err(VaultBootstrapError::VaultMismatch);
            }
            if self
                .crypto
                .is_initialized()
                .map_err(|error| VaultBootstrapError::Storage(error.to_string()))?
            {
                if !self
                    .crypto
                    .validate_bootstrap_for_active_key(master_password.clone(), &imported)?
                {
                    return Err(VaultBootstrapError::VaultMismatch);
                }
            } else {
                self.crypto
                    .derive_and_verify_bootstrap(master_password.clone(), &imported)?;
            }
        } else {
            // 配置缺失时不能信任内存里可能由一次失败解锁留下的旧版临时密钥，
            // 必须按导入参数重新派生，再用现有业务密文确认归属。
            self.crypto
                .derive_and_verify_bootstrap(master_password.clone(), &imported)?;
        }

        let local_inventory = Self::inspect_local_inventory(local_object_store).await?;
        self.verify_inventory_probe(&local_inventory)?;

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

    fn verify_remote_probe(&self, inventory: &VaultInventory) -> Result<(), VaultBootstrapError> {
        self.verify_inventory_probe(inventory)
    }

    fn verify_inventory_probe(
        &self,
        inventory: &VaultInventory,
    ) -> Result<(), VaultBootstrapError> {
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

    /// 空桶可能被两台新设备同时初始化。条件写入失败时，当前设备尚无业务数据，
    /// 因此可以用同一主密码验证并采用胜出的云端配置。
    async fn create_remote_for_new_vault(
        &self,
        bootstrap: VaultBootstrap,
        master_password: String,
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
        self.crypto
            .derive_and_verify_bootstrap(master_password, &existing)?;
        self.persist_local(existing.clone())?;
        Ok(existing)
    }

    async fn inspect_local_inventory(
        local_object_store: &LocalObjectStore,
    ) -> Result<VaultInventory, VaultBootstrapError> {
        let entries = local_object_store
            .get_all_entries()
            .await
            .map_err(storage_error)?;
        let has_objects = !entries.is_empty();
        let probe_key = entries.iter().find_map(|entry| {
            matches!(
                ObjectLocations::parse(&entry.key),
                Some(
                    StoredObject::DiaryManifest { .. }
                        | StoredObject::AiSessionMeta { .. }
                        | StoredObject::SyncedSettings
                )
            )
            .then_some(entry.key.as_str())
        });
        let encrypted_probe = match probe_key {
            Some(key) => Some(
                local_object_store
                    .get_data(key)
                    .await
                    .map_err(storage_error)?,
            ),
            None => None,
        };
        Ok(VaultInventory {
            encrypted_probe,
            has_objects,
        })
    }

    async fn inspect_remote_inventory(&self) -> Result<VaultInventory, VaultBootstrapError> {
        if self
            .oss_client
            .object_exists(ObjectLocations::synced_settings())
            .await
            .map_err(storage_error)?
        {
            return Ok(VaultInventory {
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
                return Ok(VaultInventory {
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
                return Ok(VaultInventory {
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
        Ok(VaultInventory {
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
    use crate::caches::LocalObjectStore;
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

    fn uninitialized_repository() -> VaultBootstrapRepository {
        VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            OssClient::new(),
            Crypto::new(),
        )
    }

    #[test]
    fn initializes_a_new_vault_once_with_random_parameters() {
        let first_repository = uninitialized_repository();
        let first = first_repository
            .initialize_new(
                "password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            )
            .unwrap();
        assert_eq!(first_repository.get_local(), Some(first.clone()));
        assert_ne!(first.kdf.salt, KeyDerivationParameters::legacy_debug().salt);
        assert!(matches!(
            first_repository.initialize_new(
                "password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            ),
            Err(VaultBootstrapError::AlreadyInitialized)
        ));

        let second = uninitialized_repository()
            .initialize_new(
                "password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            )
            .unwrap();
        assert_ne!(first.kdf.salt, second.kdf.salt);
        assert_ne!(first.encrypted_verifier, second.encrypted_verifier);
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
        let temp_dir = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp_dir.path().to_path_buf());

        assert_eq!(
            repository
                .import_json(&json, "password".into(), &local)
                .await
                .unwrap(),
            bootstrap
        );
        assert!(repository
            .import_json(&json, "wrong password".into(), &local)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn import_recovers_an_uninitialized_device_and_validates_local_ciphertext() {
        let source_crypto = Crypto::new();
        let source = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            OssClient::new(),
            source_crypto.clone(),
        );
        let bootstrap = source
            .initialize_new(
                "recovery password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            )
            .unwrap();
        let encrypted_manifest = source_crypto.encrypt(b"existing diary").unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp_dir.path().to_path_buf());
        local
            .save_bytes(
                &ObjectLocations::diary_manifest("8210000000000"),
                &encrypted_manifest,
            )
            .await
            .unwrap();

        let recovered_crypto = Crypto::new();
        recovered_crypto
            .derive_dek_with_parameters(
                "recovery password".into(),
                KeyDerivationParameters::legacy_debug(),
            )
            .unwrap();
        let recovered = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            OssClient::new(),
            recovered_crypto.clone(),
        );
        assert_eq!(
            recovered
                .import_json(
                    &bootstrap.to_pretty_json().unwrap(),
                    "recovery password".into(),
                    &local,
                )
                .await
                .unwrap(),
            bootstrap
        );
        assert_eq!(
            recovered_crypto.decrypt(&encrypted_manifest).unwrap(),
            b"existing diary"
        );
    }

    #[tokio::test]
    async fn import_rejects_a_valid_configuration_for_different_local_data() {
        let first_crypto = Crypto::new();
        let first = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            OssClient::new(),
            first_crypto.clone(),
        );
        first
            .initialize_new(
                "same password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            )
            .unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let local = LocalObjectStore::new(temp_dir.path().to_path_buf());
        local
            .save_bytes(
                &ObjectLocations::diary_manifest("8210000000000"),
                &first_crypto.encrypt(b"first vault diary").unwrap(),
            )
            .await
            .unwrap();

        let second = uninitialized_repository();
        let other_bootstrap = second
            .initialize_new(
                "same password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
            )
            .unwrap();
        let target = uninitialized_repository();
        assert!(matches!(
            target
                .import_json(
                    &other_bootstrap.to_pretty_json().unwrap(),
                    "same password".into(),
                    &local,
                )
                .await,
            Err(VaultBootstrapError::VaultMismatch)
        ));
        assert!(target.get_local().is_none());
    }

    #[tokio::test]
    async fn cloud_bootstrap_is_adopted_before_deriving_the_new_device_key() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let first = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            client.clone(),
            Crypto::new(),
        );
        let expected = first
            .prepare_remote(
                "shared password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
                true,
            )
            .await
            .unwrap();
        assert_ne!(
            expected.kdf.salt,
            KeyDerivationParameters::legacy_debug().salt
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
            .prepare_remote(
                "shared password".into(),
                KeyDerivationParameters::legacy_debug().memory_cost_kib,
                true,
            )
            .await
            .unwrap();
        assert_eq!(adopted, expected);
        assert_eq!(second.get_local(), Some(expected));

        guard.cleanup().await;
    }

    #[tokio::test]
    async fn old_cloud_without_bootstrap_keeps_the_legacy_profile() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let legacy_crypto = Crypto::new();
        legacy_crypto
            .derive_dek_with_parameters(
                "legacy password".into(),
                KeyDerivationParameters::legacy_current(),
            )
            .unwrap();
        let encrypted_probe = legacy_crypto.encrypt(b"legacy synced settings").unwrap();
        client
            .upload_bytes(ObjectLocations::synced_settings(), &encrypted_probe)
            .await
            .unwrap();

        let repository = VaultBootstrapRepository::new(
            AppConfigStore::in_memory(AppConfig::default()),
            client,
            Crypto::new(),
        );
        let bootstrap = repository
            .prepare_remote("legacy password".into(), 64 * 1024, true)
            .await
            .unwrap();

        assert_eq!(bootstrap.kdf, KeyDerivationParameters::legacy_current());
        assert_eq!(repository.get_local(), Some(bootstrap));
        guard.cleanup().await;
    }
}

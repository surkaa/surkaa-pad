use crate::cryptos::crypto_types::{DERIVE_SALT, KEY_LEN, MEMORY_COST_KIB, NONCE_LEN};
use crate::error::AppError;
use argon2::password_hash::SaltString;
use argon2::ParamsBuilder;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

pub const VAULT_BOOTSTRAP_SCHEMA_VERSION: u32 = 1;
pub const ARGON2_VERSION_13: u32 = 0x13;
pub const LEGACY_TIME_COST: u32 = 2;
pub const LEGACY_PARALLELISM: u32 = 4;
pub const LEGACY_DEBUG_MEMORY_COST_KIB: u32 = 1024;
pub const LEGACY_RELEASE_MEMORY_COST_KIB: u32 = 256 * 1024;
pub const VAULT_VERIFIER_TEXT: &str = "surkaa-pad:vault-verifier:v1";

const NEW_VAULT_SALT_BYTES: usize = 16;
const MAX_MEMORY_COST_KIB: u32 = 256 * 1024;
const MAX_TIME_COST: u32 = 10;
const MAX_PARALLELISM: u32 = 8;
const MAX_VERIFIER_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum KeyDerivationAlgorithm {
    Argon2id,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyDerivationParameters {
    pub algorithm: KeyDerivationAlgorithm,
    pub algorithm_version: u32,
    pub salt: String,
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub output_length: u32,
}

impl KeyDerivationParameters {
    pub fn new_random(memory_cost_kib: u32) -> Result<Self, VaultBootstrapError> {
        let mut salt_bytes = [0_u8; NEW_VAULT_SALT_BYTES];
        getrandom::fill(&mut salt_bytes)
            .map_err(|error| VaultBootstrapError::Storage(error.to_string()))?;
        let parameters = Self {
            algorithm: KeyDerivationAlgorithm::Argon2id,
            algorithm_version: ARGON2_VERSION_13,
            salt: SaltString::encode_b64(&salt_bytes)
                .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?
                .to_string(),
            memory_cost_kib,
            time_cost: LEGACY_TIME_COST,
            parallelism: LEGACY_PARALLELISM,
            output_length: KEY_LEN as u32,
        };
        parameters.validate()?;
        Ok(parameters)
    }

    pub fn legacy_current() -> Self {
        Self::legacy_with_memory_cost(MEMORY_COST_KIB)
    }

    pub fn legacy_debug() -> Self {
        Self::legacy_with_memory_cost(LEGACY_DEBUG_MEMORY_COST_KIB)
    }

    pub fn legacy_release() -> Self {
        Self::legacy_with_memory_cost(LEGACY_RELEASE_MEMORY_COST_KIB)
    }

    pub fn legacy_candidates() -> [Self; 2] {
        if cfg!(debug_assertions) {
            [Self::legacy_debug(), Self::legacy_release()]
        } else {
            [Self::legacy_release(), Self::legacy_debug()]
        }
    }

    fn legacy_with_memory_cost(memory_cost_kib: u32) -> Self {
        Self {
            algorithm: KeyDerivationAlgorithm::Argon2id,
            algorithm_version: ARGON2_VERSION_13,
            salt: DERIVE_SALT.to_owned(),
            memory_cost_kib,
            time_cost: LEGACY_TIME_COST,
            parallelism: LEGACY_PARALLELISM,
            output_length: KEY_LEN as u32,
        }
    }

    pub fn validate(&self) -> Result<(), VaultBootstrapError> {
        if self.algorithm_version != ARGON2_VERSION_13 {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "暂不支持 Argon2 版本 {}",
                self.algorithm_version
            )));
        }
        SaltString::from_b64(&self.salt)
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        if self.memory_cost_kib < LEGACY_DEBUG_MEMORY_COST_KIB
            || self.memory_cost_kib > MAX_MEMORY_COST_KIB
        {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "内存成本必须在 {}–{} KiB 之间",
                LEGACY_DEBUG_MEMORY_COST_KIB, MAX_MEMORY_COST_KIB
            )));
        }
        if !(1..=MAX_TIME_COST).contains(&self.time_cost) {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "时间成本必须在 1–{MAX_TIME_COST} 之间"
            )));
        }
        if !(1..=MAX_PARALLELISM).contains(&self.parallelism) {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "并行度必须在 1–{MAX_PARALLELISM} 之间"
            )));
        }
        if self.output_length != KEY_LEN as u32 {
            return Err(VaultBootstrapError::InvalidConfiguration(format!(
                "派生密钥长度必须为 {KEY_LEN} 字节"
            )));
        }
        ParamsBuilder::new()
            .m_cost(self.memory_cost_kib)
            .t_cost(self.time_cost)
            .p_cost(self.parallelism)
            .output_len(self.output_length as usize)
            .build()
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VaultBootstrap {
    pub schema_version: u32,
    pub vault_id: String,
    pub kdf: KeyDerivationParameters,
    pub encrypted_verifier: String,
}

impl VaultBootstrap {
    pub fn validate(&self) -> Result<(), VaultBootstrapError> {
        if self.schema_version != VAULT_BOOTSTRAP_SCHEMA_VERSION {
            return Err(VaultBootstrapError::UnsupportedVersion(self.schema_version));
        }
        if self.vault_id.len() != 32
            || !self
                .vault_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(VaultBootstrapError::InvalidConfiguration(
                "Vault ID 必须是 32 位小写十六进制字符串".into(),
            ));
        }
        self.kdf.validate()?;
        let verifier = self.decode_verifier()?;
        if verifier.len() < NONCE_LEN + 16 || verifier.len() > MAX_VERIFIER_BYTES {
            return Err(VaultBootstrapError::InvalidConfiguration(
                "加密校验值长度不正确".into(),
            ));
        }
        Ok(())
    }

    pub fn decode_verifier(&self) -> Result<Vec<u8>, VaultBootstrapError> {
        STANDARD
            .decode(&self.encrypted_verifier)
            .map_err(|error| VaultBootstrapError::InvalidConfiguration(error.to_string()))
    }

    pub fn same_vault_definition(&self, other: &Self) -> bool {
        self.vault_id == other.vault_id && self.kdf == other.kdf
    }

    pub fn to_pretty_json(&self) -> Result<String, VaultBootstrapError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, VaultBootstrapError> {
        let value: Self = serde_json::from_str(json)?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Debug, Error)]
pub enum VaultBootstrapError {
    #[error("解析密钥派生配置失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("密钥派生配置无效: {0}")]
    InvalidConfiguration(String),
    #[error("不支持的密钥派生配置版本: {0}")]
    UnsupportedVersion(u32),
    #[error("密钥派生配置与当前 Vault 不匹配")]
    VaultMismatch,
    #[error("密钥派生配置的校验值无法通过验证")]
    VerifierMismatch,
    #[error("尚未建立密钥派生配置")]
    NotInitialized,
    #[error("当前 Vault 已经建立密钥派生配置，不能重新初始化")]
    AlreadyInitialized,
    #[error("检测到已有本地对象，不能按新 Vault 生成随机密钥派生参数")]
    ExistingLocalData,
    #[error("读取或保存密钥派生配置失败: {0}")]
    Storage(String),
}

impl From<VaultBootstrapError> for AppError {
    fn from(error: VaultBootstrapError) -> Self {
        Self {
            error_type: "vault_bootstrap".into(),
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_bootstrap() -> VaultBootstrap {
        VaultBootstrap {
            schema_version: VAULT_BOOTSTRAP_SCHEMA_VERSION,
            vault_id: "00112233445566778899aabbccddeeff".into(),
            kdf: KeyDerivationParameters::legacy_debug(),
            encrypted_verifier: STANDARD.encode(vec![0; NONCE_LEN + 16]),
        }
    }

    #[test]
    fn legacy_profile_keeps_build_specific_memory_cost() {
        assert_eq!(
            KeyDerivationParameters::legacy_current().memory_cost_kib,
            MEMORY_COST_KIB
        );
        assert_eq!(
            KeyDerivationParameters::legacy_debug().memory_cost_kib,
            LEGACY_DEBUG_MEMORY_COST_KIB
        );
        assert_eq!(
            KeyDerivationParameters::legacy_release().memory_cost_kib,
            LEGACY_RELEASE_MEMORY_COST_KIB
        );
    }

    #[test]
    fn new_vault_profiles_use_independent_random_salts() {
        let first = KeyDerivationParameters::new_random(LEGACY_DEBUG_MEMORY_COST_KIB).unwrap();
        let second = KeyDerivationParameters::new_random(LEGACY_DEBUG_MEMORY_COST_KIB).unwrap();

        assert_ne!(first.salt, second.salt);
        assert_ne!(first.salt, DERIVE_SALT);
        assert_eq!(first.memory_cost_kib, LEGACY_DEBUG_MEMORY_COST_KIB);
        first.validate().unwrap();
        second.validate().unwrap();
    }

    #[test]
    fn bootstrap_json_roundtrips() {
        let bootstrap = valid_bootstrap();
        let json = bootstrap.to_pretty_json().unwrap();
        assert_eq!(VaultBootstrap::from_json(&json).unwrap(), bootstrap);
    }

    #[test]
    fn rejects_resource_exhaustion_parameters_before_derivation() {
        let mut bootstrap = valid_bootstrap();
        bootstrap.kdf.memory_cost_kib = MAX_MEMORY_COST_KIB + 1;
        assert!(matches!(
            bootstrap.validate(),
            Err(VaultBootstrapError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn rejects_invalid_vault_id_and_verifier() {
        let mut bootstrap = valid_bootstrap();
        bootstrap.vault_id = "UPPERCASE".into();
        assert!(bootstrap.validate().is_err());

        let mut bootstrap = valid_bootstrap();
        bootstrap.encrypted_verifier = "not base64".into();
        assert!(bootstrap.validate().is_err());
    }
}

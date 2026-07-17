use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("RwLock poisoned")]
    LockPoisoned,

    #[error("Key not derived yet")]
    KeyNotDerived,

    #[error("Ciphertext too short to contain nonce")]
    CiphertextTooShort,

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Invalid salt: {0}")]
    InvalidSalt(String),

    #[error("Nonce length error: expected {expected}, got {actual}")]
    InvalidNonceLength { expected: usize, actual: usize },

    #[error("Key derivation failed: {0}")]
    DeriveFailed(String),

    #[error("Invalid DEK hex: {0}")]
    InvalidDekHex(String),

    #[error("Invalid DEK length: expected {expected}, got {actual}")]
    InvalidDekLength { expected: usize, actual: usize },

    #[error("Password does not match")]
    PasswordMismatch,

    #[error("Crypto not initialized")]
    NotInitialized,
}

impl From<CryptoError> for crate::error::AppError {
    fn from(e: CryptoError) -> Self {
        crate::error::AppError {
            error_type: "crypto".into(),
            message: e.to_string(),
        }
    }
}

mod command;
mod diagnostics;
mod engine;
mod inventory;
mod journal;
mod progress;
mod storage;
mod stream_digest;
mod types;

pub use command::*;
pub(crate) use diagnostics::KeyChangeReport;

use crate::error::AppError;
use std::fmt::Display;

type Result<T> = std::result::Result<T, AppError>;

fn error(message: impl Display) -> AppError {
    AppError {
        error_type: "vault_key_change".into(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests;

mod command;
mod engine;
mod inventory;
mod journal;
mod storage;
mod types;

pub use command::*;

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

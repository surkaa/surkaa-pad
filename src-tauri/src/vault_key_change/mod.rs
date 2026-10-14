// 先独立验证协调协议；后续接入开始、续改和确认三个业务入口。
#[cfg(test)]
mod cloud_claim;
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

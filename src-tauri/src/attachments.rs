mod attachment;
pub mod attachment_command;
mod attachment_protocol;
mod types;

#[cfg(test)]
mod attachment_tests;

pub use attachment_protocol::{attachment_protocol, get_full_attachment_url, PROTOCOL_NAME};
pub use types::AttachmentMeta;

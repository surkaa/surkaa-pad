mod attachment;
pub mod attachment_command;
pub mod chunked_upload;
mod attachment_error;
mod attachment_protocol;
mod attachment_types;

#[cfg(test)]
mod attachment_tests;

pub use attachment_error::AttachmentError;
pub use attachment_protocol::{attachment_protocol, get_full_attachment_url, PROTOCOL_NAME};
pub use attachment_types::AttachmentMeta;

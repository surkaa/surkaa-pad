mod attachment;
mod attachment_protocol;
pub mod command;
mod types;

pub use attachment_protocol::{attachment_protocol, get_full_attachment_url, PROTOCOL_NAME};
pub use types::AttachmentMeta;

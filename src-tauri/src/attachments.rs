mod attachment;
pub mod attachment_command;
mod attachment_error;
mod attachment_server;
mod attachment_types;
pub mod chunked_upload;
pub mod chunked_upload_command;
mod embedded_media;

#[cfg(test)]
mod attachment_tests;
#[cfg(test)]
mod embedded_media_tests;

pub use attachment_error::AttachmentError;
pub use attachment_server::{
    bind_attachment_server, start_attachment_server, AttachmentServerHandle,
};
pub use attachment_types::{
    AttachmentContentInfo, AttachmentMeta, AudioWaveform, SharedAttachmentSource,
};

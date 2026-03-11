use bytes::Bytes;
use futures::Stream;
use std::io::Error;
use std::pin::Pin;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub type NextToken = Option<String>;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>>;

pub(super) const STREAM_MINE_TYPE: &str = "application/octet-stream";

// 附件URL过期时间，单位秒
pub(super) const ATTACHMENT_URL_EXPIRATION_SECONDS: i64 = 3600;

#[derive(Debug, Eq, PartialEq)]
pub struct ObjectMetadata {
    key: String,
    size: u64,
    last_modified: chrono::DateTime<Utc>,
    etag: String,
}

impl ObjectMetadata {
    pub fn new(key: String, size: u64, last_modified: chrono::DateTime<Utc>, etag: String) -> Self {
        Self {
            key,
            size,
            last_modified,
            etag,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn etag(&self) -> &str {
        &self.etag
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    #[cfg(test)]
    pub fn last_modified(&self) -> chrono::DateTime<Utc> {
        self.last_modified
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct AliyunListObjectsResult {
    pub name: String,
    pub prefix: Option<String>,
    // pub marker: Option<String>,
    pub max_keys: i32,
    // pub is_truncated: bool,
    #[serde(rename = "Contents", default)] // 避免没有对象时反序列化失败
    pub contents: Vec<AliyunObjectSummary>,
    pub next_continuation_token: Option<String>,
}

impl AliyunListObjectsResult {
    pub fn from_xml(xml: String) -> Result<Self, quick_xml::DeError> {
        quick_xml::de::from_str(&xml)
    }
}

/// List Objects 中的单个对象摘要
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct AliyunObjectSummary {
    pub key: String,
    pub last_modified: String,
    pub e_tag: String,
    pub size: u64,
    pub storage_class: String,
}

impl AliyunObjectSummary {
    pub(super) fn to_object_metadata(self) -> ObjectMetadata {
        ObjectMetadata {
            key: self.key,
            size: self.size,
            last_modified: chrono::DateTime::parse_from_rfc3339(&self.last_modified)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            etag: self.e_tag.replace("\"", ""),
        }
    }
}

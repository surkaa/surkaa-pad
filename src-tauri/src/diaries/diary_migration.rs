use crate::diaries::{DiaryContent, DiaryError, DiaryStore};
use crate::object::ObjectMigrationOutcome;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tauri_plugin_log::log;

/// 代码当前支持的 schema 版本。
pub const CURRENT_VERSION: u32 = 4;

/// 迁移步骤可以通过上下文操作当前日记所属的存储。
pub struct MigrationContext<'a> {
    pub diary_id: &'a str,
    pub store: &'a dyn DiaryStore,
}

/// 单个迁移步骤：将 JSON 从 V(n) 转换为 V(n+1)。
#[async_trait]
pub trait DiaryMigration: Send + Sync {
    fn source_version(&self) -> u32;

    async fn migrate_json(
        &self,
        context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError>;

    #[cfg(test)]
    fn test_input(&self) -> Value {
        let mut json = Value::Object(serde_json::Map::new());
        json["version"] = Value::Number(self.source_version().into());
        json
    }

    #[cfg(test)]
    fn test_verify(&self, _json: &Value) {}
}

/// 迁移步骤的有序注册表。
pub struct MigrationRegistry {
    steps: Vec<Box<dyn DiaryMigration>>,
}

impl MigrationRegistry {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn register(&mut self, step: Box<dyn DiaryMigration>) {
        self.steps.push(step);
    }

    #[cfg(test)]
    pub fn steps(&self) -> &[Box<dyn DiaryMigration>] {
        &self.steps
    }

    /// 按序执行所有适用迁移。只有步骤成功后才提升 JSON version。
    pub async fn migrate(
        &self,
        context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<bool, DiaryError> {
        let version = get_version(json);
        if version > CURRENT_VERSION {
            return Err(DiaryError::UnsupportedVersion {
                found: version,
                supported: CURRENT_VERSION,
            });
        }
        if version == CURRENT_VERSION {
            log::debug!(
                "Manifest version {version} already current (latest {CURRENT_VERSION}), skip migration"
            );
            return Ok(false);
        }

        let before = version;
        for step in &self.steps {
            let current = get_version(json);
            if current == step.source_version() {
                log::info!(
                    "Migrating manifest: V{} → V{}",
                    current,
                    step.source_version() + 1
                );
                step.migrate_json(context, json).await?;
                json["version"] = Value::Number((step.source_version() + 1).into());
            }
        }

        let after = get_version(json);
        if after != CURRENT_VERSION {
            return Err(DiaryError::InvalidManifest(format!(
                "No complete migration path from V{before} to V{CURRENT_VERSION}"
            )));
        }
        log::info!("Manifest migration complete: V{before} → V{after}");
        Ok(after != before)
    }
}

/// 从 JSON 中读取 version，缺省按 V1 处理。
pub fn get_version(json: &Value) -> u32 {
    json.get("version").and_then(Value::as_u64).unwrap_or(1) as u32
}

/// V1 → V2：为每个附件添加 etag 字段。
struct V1ToV2Migration;

#[async_trait]
impl DiaryMigration for V1ToV2Migration {
    fn source_version(&self) -> u32 {
        1
    }

    async fn migrate_json(
        &self,
        _context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError> {
        let mut count = 0;
        if let Some(attachments) = json.get_mut("attachments").and_then(Value::as_array_mut) {
            for attachment in attachments {
                if attachment.get("etag").is_none() {
                    attachment["etag"] = Value::Null;
                    count += 1;
                }
            }
        }
        log::debug!("V1→V2: 为 {count} 个附件注入 etag 字段");
        Ok(())
    }

    #[cfg(test)]
    fn test_input(&self) -> Value {
        serde_json::json!({
            "id": "test-v1",
            "content": "V1 diary content",
            "attachments": [
                {
                    "filename": "1",
                    "mimetype": "image/png",
                    "size": 1024,
                    "encrypted": true,
                    "nonce": [],
                    "algorithm": "AES-256-CTR"
                },
                {
                    "filename": "2",
                    "mimetype": "audio/mp3",
                    "size": 2048,
                    "encrypted": false,
                    "nonce": [],
                    "algorithm": "AES-256-CTR"
                }
            ]
        })
    }

    #[cfg(test)]
    fn test_verify(&self, json: &Value) {
        for attachment in json["attachments"].as_array().unwrap() {
            assert_eq!(attachment["etag"], Value::Null);
        }
    }
}

/// V2 → V3：将 Markdown + 附件标记转换为有序结构化节点。
struct V2ToV3Migration;

#[async_trait]
impl DiaryMigration for V2ToV3Migration {
    fn source_version(&self) -> u32 {
        2
    }

    async fn migrate_json(
        &self,
        _context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError> {
        let content = json.get("content").and_then(Value::as_str).ok_or_else(|| {
            DiaryError::InvalidManifest("V2 manifest content must be a string".to_string())
        })?;
        let mut content = DiaryContent::from_editor_text(content);
        content.group_consecutive_images_into_albums();
        let mut content_json = serde_json::to_value(content)?;
        // DiaryContent 已经是 V4 Rust 类型，这里显式写回历史 V3 字段，
        // 保证每个迁移步骤仍准确表达自己的目标版本。
        convert_content_references_to_v3(&mut content_json)?;
        json["content"] = content_json;
        Ok(())
    }

    #[cfg(test)]
    fn test_input(&self) -> Value {
        serde_json::json!({
            "id": "test-v2",
            "version": 2,
            "content": "开头\n\n[[IMG:1.jpg|size=small]]\n\n[[IMG:2.jpg]]\n\n结尾",
            "attachments": []
        })
    }

    #[cfg(test)]
    fn test_verify(&self, json: &Value) {
        assert_eq!(json["content"]["nodes"][0]["type"], "markdown");
        assert_eq!(json["content"]["nodes"][1]["type"], "album");
        assert_eq!(json["content"]["nodes"][1]["id"], "migration-v3-album-1");
        assert_eq!(
            json["content"]["nodes"][1]["attachmentIds"],
            serde_json::json!([
                legacy_attachment_id("test-v2", "1.jpg"),
                legacy_attachment_id("test-v2", "2.jpg")
            ])
        );
        assert_eq!(json["content"]["nodes"][1]["displayMode"], "horizontalList");
    }
}

fn convert_content_references_to_v3(content: &mut Value) -> Result<(), DiaryError> {
    let nodes = content
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| DiaryError::InvalidManifest("content.nodes must be an array".to_string()))?;
    for node in nodes {
        let object = node.as_object_mut().ok_or_else(|| {
            DiaryError::InvalidManifest("content node must be an object".to_string())
        })?;
        match object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "image" | "video" | "audio" | "file" => {
                if let Some(reference) = object.remove("attachmentId") {
                    object.insert("filename".to_string(), reference);
                }
            }
            "album" => {
                if let Some(references) = object.remove("attachmentIds") {
                    object.insert("images".to_string(), references);
                }
            }
            "markdown" => {}
            kind => {
                return Err(DiaryError::InvalidManifest(format!(
                    "Unknown content node type: {kind}"
                )));
            }
        }
    }
    Ok(())
}

/// V3 → V4：filename 仅作为展示名，附件 ID 同时作为物理 object key。
struct V3ToV4Migration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct LegacyObjectMigration {
    old_filename: String,
    attachment_id: String,
}

#[async_trait]
impl DiaryMigration for V3ToV4Migration {
    fn source_version(&self) -> u32 {
        3
    }

    async fn migrate_json(
        &self,
        context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError> {
        let manifest_diary_id = json
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| DiaryError::InvalidManifest("V3 manifest id is missing".to_string()))?;
        if manifest_diary_id != context.diary_id {
            return Err(DiaryError::InvalidManifest(format!(
                "Manifest diary id {manifest_diary_id} does not match requested id {}",
                context.diary_id
            )));
        }

        // 先在副本上完成纯转换，存储迁移失败时不会向调用者暴露半成品 JSON。
        let mut migrated_json = json.clone();
        let object_migrations = transform_v3_to_v4(&mut migrated_json, context.diary_id)?;
        for migration in object_migrations {
            let outcome = context
                .store
                .migrate_attachment_object(
                    context.diary_id,
                    &migration.old_filename,
                    &migration.attachment_id,
                )
                .await?;
            if outcome == ObjectMigrationOutcome::Missing {
                // 元数据已经引用了不存在的附件时仍允许升级，避免永久阻塞整篇日记。
                log::warn!(
                    "V3→V4 attachment missing: diary={}, filename={}, attachment_id={}",
                    context.diary_id,
                    migration.old_filename,
                    migration.attachment_id
                );
            }
        }
        *json = migrated_json;
        Ok(())
    }

    #[cfg(test)]
    fn test_input(&self) -> Value {
        v3_test_input()
    }

    #[cfg(test)]
    fn test_verify(&self, json: &Value) {
        let photo_id = legacy_attachment_id("test-v3", "photo.jpg");
        assert_eq!(json["attachments"][0]["id"], photo_id);
        assert_eq!(json["content"]["nodes"][0]["attachmentId"], photo_id);
        assert!(json["content"]["nodes"][0].get("filename").is_none());
        assert_eq!(
            json["content"]["nodes"][2]["attachmentIds"],
            serde_json::json!([
                legacy_attachment_id("test-v3", "photo.jpg"),
                legacy_attachment_id("test-v3", "second.jpg")
            ])
        );
        assert!(json["content"]["nodes"][2].get("images").is_none());
    }
}

fn transform_v3_to_v4(
    json: &mut Value,
    diary_id: &str,
) -> Result<Vec<LegacyObjectMigration>, DiaryError> {
    let attachments = json
        .get_mut("attachments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            DiaryError::InvalidManifest("V3 manifest attachments must be an array".to_string())
        })?;
    let mut ids_by_filename = HashMap::with_capacity(attachments.len());
    let mut migrations = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let filename = attachment
            .get("filename")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DiaryError::InvalidManifest("V3 attachment filename must be a string".to_string())
            })?
            .to_string();
        let attachment_id = legacy_attachment_id(diary_id, &filename);
        attachment["id"] = Value::String(attachment_id.clone());
        if ids_by_filename
            .insert(filename.clone(), attachment_id.clone())
            .is_some()
        {
            return Err(DiaryError::InvalidManifest(format!(
                "V3 manifest contains duplicate attachment filename: {filename}"
            )));
        }
        migrations.push(LegacyObjectMigration {
            old_filename: filename,
            attachment_id,
        });
    }

    let nodes = json
        .get_mut("content")
        .and_then(|content| content.get_mut("nodes"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            DiaryError::InvalidManifest("V3 manifest content.nodes must be an array".to_string())
        })?;
    for node in nodes {
        migrate_content_node(node, diary_id, &ids_by_filename)?;
    }
    Ok(migrations)
}

fn migrate_content_node(
    node: &mut Value,
    diary_id: &str,
    ids_by_filename: &HashMap<String, String>,
) -> Result<(), DiaryError> {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();
    match node_type {
        "image" | "video" | "audio" | "file" => {
            let filename = node
                .get("filename")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DiaryError::InvalidManifest(format!("V3 {node_type} node filename is missing"))
                })?
                .to_string();
            let attachment_id = ids_by_filename
                .get(&filename)
                .cloned()
                .unwrap_or_else(|| legacy_attachment_id(diary_id, &filename));
            let object = node.as_object_mut().unwrap();
            object.remove("filename");
            object.insert("attachmentId".to_string(), Value::String(attachment_id));
        }
        "album" => {
            let filenames = node
                .get("images")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiaryError::InvalidManifest("V3 album images must be an array".to_string())
                })?
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        DiaryError::InvalidManifest(
                            "V3 album image reference must be a string".to_string(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let attachment_ids = filenames
                .iter()
                .map(|filename| {
                    ids_by_filename
                        .get(filename)
                        .cloned()
                        .unwrap_or_else(|| legacy_attachment_id(diary_id, filename))
                })
                .map(Value::String)
                .collect();
            let object = node.as_object_mut().unwrap();
            object.remove("images");
            object.insert("attachmentIds".to_string(), Value::Array(attachment_ids));
        }
        "markdown" => {}
        _ => {
            return Err(DiaryError::InvalidManifest(format!(
                "Unknown V3 content node type: {node_type}"
            )));
        }
    }
    Ok(())
}

pub fn legacy_attachment_id(diary_id: &str, filename: &str) -> String {
    let digest = md5::compute(format!("{diary_id}\0{filename}"));
    format!("att-{digest:x}")
}

#[cfg(test)]
fn v3_test_input() -> Value {
    serde_json::json!({
        "id": "test-v3",
        "version": 3,
        "content": {
            "nodes": [
                {"type": "image", "filename": "photo.jpg", "size": "normal"},
                {"type": "file", "filename": "notes.txt"},
                {
                    "type": "album",
                    "id": "album-1",
                    "images": ["photo.jpg", "second.jpg"],
                    "displayMode": "horizontalList"
                }
            ]
        },
        "attachments": [
            {"filename": "photo.jpg"},
            {"filename": "notes.txt"},
            {"filename": "second.jpg"}
        ]
    })
}

pub fn default_registry() -> MigrationRegistry {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(V1ToV2Migration));
    registry.register(Box::new(V2ToV3Migration));
    registry.register(Box::new(V3ToV4Migration));
    registry
}

/// 迁移 manifest JSON。物理对象迁移在步骤内部通过 MigrationContext 执行，
/// 返回 Some 时，调用方仍须最后上传这些字节，完成 V4 的提交。
pub async fn migrate_manifest_bytes(
    context: &MigrationContext<'_>,
    manifest_bytes: &[u8],
) -> Result<Option<Vec<u8>>, DiaryError> {
    let mut json: Value = serde_json::from_slice(manifest_bytes)?;
    if default_registry().migrate(context, &mut json).await? {
        Ok(Some(serde_json::to_vec(&json)?))
    } else {
        Ok(None)
    }
}

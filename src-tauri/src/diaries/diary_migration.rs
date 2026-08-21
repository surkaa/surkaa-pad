use crate::diaries::diary_types::{inspect_manifest_json, CURRENT_VERSION};
use crate::diaries::{DiaryError, DiaryStore};
use async_trait::async_trait;
use serde_json::Value;

/// 迁移步骤可以通过上下文操作当前日记所属的存储。
pub(crate) struct MigrationContext<'a> {
    pub diary_id: &'a str,
    pub store: &'a dyn DiaryStore,
}

/// 单个迁移步骤：将 JSON 从 V(n) 转换为 V(n+1)。
#[async_trait]
trait DiaryMigration: Send + Sync {
    fn source_version(&self) -> u32;

    async fn migrate_json(
        &self,
        context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError>;
}

struct MigrationRegistry {
    steps: Vec<Box<dyn DiaryMigration>>,
}

struct V4ToV5;
struct V5ToV6;

#[async_trait]
impl DiaryMigration for V4ToV5 {
    fn source_version(&self) -> u32 {
        4
    }

    async fn migrate_json(
        &self,
        _context: &MigrationContext<'_>,
        _json: &mut Value,
    ) -> Result<(), DiaryError> {
        // V5 只新增可选的 Summary 内容节点，现有 V4 内容无需改写。
        Ok(())
    }
}

#[async_trait]
impl DiaryMigration for V5ToV6 {
    fn source_version(&self) -> u32 {
        5
    }

    async fn migrate_json(
        &self,
        _context: &MigrationContext<'_>,
        _json: &mut Value,
    ) -> Result<(), DiaryError> {
        // V6 新增位置内容节点；已有 V5 节点结构保持不变。
        Ok(())
    }
}

impl MigrationRegistry {
    fn new(steps: Vec<Box<dyn DiaryMigration>>) -> Self {
        Self { steps }
    }

    async fn migrate(
        &self,
        context: &MigrationContext<'_>,
        json: &mut Value,
        mut version: u32,
    ) -> Result<bool, DiaryError> {
        if version > CURRENT_VERSION {
            return Err(DiaryError::UnsupportedVersion {
                found: version,
                supported: CURRENT_VERSION,
            });
        }
        if version == CURRENT_VERSION {
            return Ok(false);
        }

        let original_version = version;
        for step in &self.steps {
            if version == step.source_version() {
                step.migrate_json(context, json).await?;
                version = version.saturating_add(1);
                json["version"] = Value::Number(version.into());
            }
        }

        if version != CURRENT_VERSION {
            return Err(DiaryError::UnsupportedVersion {
                found: original_version,
                supported: CURRENT_VERSION,
            });
        }
        Ok(true)
    }
}

fn default_registry() -> MigrationRegistry {
    // V1–V3 的兼容步骤已移除，只保留当前仍需执行的连续迁移步骤。
    MigrationRegistry::new(vec![Box::new(V4ToV5), Box::new(V5ToV6)])
}

/// 检查并按注册步骤迁移 Manifest；返回 Some 时由调用方最后提交新版 Manifest。
pub(crate) async fn migrate_manifest_bytes(
    context: &MigrationContext<'_>,
    manifest_bytes: &[u8],
) -> Result<Option<Vec<u8>>, DiaryError> {
    // 保持存储感知迁移的上下文契约；并非每个版本步骤都需要移动关联对象。
    let _store = context.store;
    let (mut json, version) = inspect_manifest_json(context.diary_id, manifest_bytes)?;
    if default_registry()
        .migrate(context, &mut json, version)
        .await?
    {
        Ok(Some(serde_json::to_vec(&json)?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::LocalObjectStore;
    use crate::diaries::diary_types::deserialize_current_manifest;
    use crate::diaries::LocalStore;

    #[tokio::test]
    async fn current_version_passes_through_and_supported_versions_migrate_without_changing_content(
    ) {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: "test",
            store: &store,
        };

        let current = serde_json::json!({
            "id": "test",
            "version": CURRENT_VERSION,
            "content": {"nodes": []},
            "attachments": [],
        });
        assert!(
            migrate_manifest_bytes(&context, &serde_json::to_vec(&current).unwrap())
                .await
                .unwrap()
                .is_none()
        );

        let v4 = serde_json::json!({
            "id": "test",
            "version": 4,
            "algorithm": "AES256-GCM_v1",
            "content": {"nodes": [{"type": "markdown", "text": "正文"}]},
            "created": 1,
            "updated": 2,
            "attachments": [],
        });
        let migrated = migrate_manifest_bytes(&context, &serde_json::to_vec(&v4).unwrap())
            .await
            .unwrap()
            .unwrap();
        let migrated: Value = serde_json::from_slice(&migrated).unwrap();
        assert_eq!(migrated["version"], CURRENT_VERSION);
        assert_eq!(migrated["content"], v4["content"]);
        assert_eq!(
            deserialize_current_manifest("test", &serde_json::to_vec(&migrated).unwrap())
                .unwrap()
                .content
                .searchable_text(),
            "正文"
        );

        let v5 = serde_json::json!({
            "id": "test",
            "version": 5,
            "algorithm": "AES256-GCM_v1",
            "content": {"nodes": [{"type": "summary", "summary": "标题", "content": "内容"}]},
            "created": 1,
            "updated": 2,
            "attachments": [],
        });
        let migrated = migrate_manifest_bytes(&context, &serde_json::to_vec(&v5).unwrap())
            .await
            .unwrap()
            .unwrap();
        let migrated: Value = serde_json::from_slice(&migrated).unwrap();
        assert_eq!(migrated["version"], CURRENT_VERSION);
        assert_eq!(migrated["content"], v5["content"]);

        for version in 1..4 {
            let source = serde_json::json!({"id": "test", "version": version});
            assert!(matches!(
                migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap()).await,
                Err(DiaryError::UnsupportedVersion { found, supported })
                    if found == version && supported == CURRENT_VERSION
            ));
        }
    }
}

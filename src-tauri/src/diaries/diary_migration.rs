use crate::diaries::diary_types::{inspect_manifest_json, CURRENT_VERSION};
use crate::diaries::{DiaryError, DiaryStore};
use async_trait::async_trait;
use serde_json::Value;

/// 迁移步骤可以通过上下文操作当前日记所属的存储。
pub(crate) struct MigrationContext<'a> {
    pub diary_id: u64,
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
    // V1–V3 的兼容步骤已移除；V4→V5 将 Manifest 主键从数字字符串转为 u64 数字。
    MigrationRegistry::new(vec![Box::new(V5StringIdToNumber)])
}

/// V4→V5：Manifest 主键从数字字符串改为 u64 数字。
struct V5StringIdToNumber;

#[async_trait]
impl DiaryMigration for V5StringIdToNumber {
    fn source_version(&self) -> u32 {
        4
    }

    async fn migrate_json(
        &self,
        _context: &MigrationContext<'_>,
        json: &mut Value,
    ) -> Result<(), DiaryError> {
        let id = json
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DiaryError::InvalidManifest("V4 manifest id must be a numeric string".into())
            })?
            .parse::<u64>()
            .map_err(|_| {
                DiaryError::InvalidManifest("V4 manifest id must be a numeric string".into())
            })?;
        json["id"] = Value::Number(id.into());
        Ok(())
    }
}

/// 检查并按注册步骤迁移 Manifest；返回 Some 时由调用方最后提交新版 Manifest。
pub(crate) async fn migrate_manifest_bytes(
    context: &MigrationContext<'_>,
    manifest_bytes: &[u8],
) -> Result<Option<Vec<u8>>, DiaryError> {
    // 即使当前没有存储感知的迁移步骤，也保持上下文契约经过编译；未来步骤可直接操作存储。
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
    use crate::diaries::LocalStore;

    #[tokio::test]
    async fn current_version_passes_through_and_legacy_versions_have_no_compatibility_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let context = MigrationContext {
            diary_id: 1,
            store: &store,
        };

        assert!(migrate_manifest_bytes(
            &context,
            br#"{"id":1,"version":5,"content":{"nodes":[]},"attachments":[]}"#,
        )
        .await
        .unwrap()
        .is_none());

        for version in 1..(CURRENT_VERSION - 1) {
            let source = serde_json::json!({"id": 1, "version": version});
            assert!(matches!(
                migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap()).await,
                Err(DiaryError::UnsupportedVersion { found, supported })
                    if found == version && supported == CURRENT_VERSION
            ));
        }
    }

    #[tokio::test]
    async fn v4_string_id_manifest_migrates_to_numeric_u64_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LocalStore::new(LocalObjectStore::new(temp_dir.path().to_path_buf()));
        let diary_id = 8_215_021_834_823u64;
        let context = MigrationContext {
            diary_id,
            store: &store,
        };
        let source = serde_json::json!({
            "id": "8215021834823",
            "algorithm": "AES256-GCM_v1",
            "content": {"nodes": []},
            "created": 1,
            "updated": 1,
            "attachments": [],
            "version": 4,
        });

        let migrated = migrate_manifest_bytes(&context, &serde_json::to_vec(&source).unwrap())
            .await
            .unwrap()
            .expect("V4 manifest 应被迁移到 V5");

        let json: Value = serde_json::from_slice(&migrated).unwrap();
        assert_eq!(json["id"], diary_id);
        assert_eq!(json["version"], CURRENT_VERSION);

        let manifest: crate::diaries::DiaryManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.id, diary_id);
        assert_eq!(manifest.version, CURRENT_VERSION);
    }
}

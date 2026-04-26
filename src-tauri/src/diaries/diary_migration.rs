use crate::diaries::DiaryError;
use serde_json::Value;

/// 代码当前支持的 schema 版本
pub const CURRENT_VERSION: u32 = 1;

/// 单个迁移步骤：将 JSON 从 V(n) 转换为 V(n+1)
pub trait DiaryMigration: Send + Sync {
    /// 此迁移步骤处理的源版本
    fn source_version(&self) -> u32;
    /// 在 JSON 值上执行迁移（就地修改）
    fn migrate_json(&self, json: &mut Value) -> Result<(), DiaryError>;
}

/// 迁移步骤的有序注册表
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

    /// 按序执行所有适用的迁移步骤，返回是否发生了迁移
    pub fn migrate(&self, json: &mut Value) -> Result<bool, DiaryError> {
        let version = get_version(json);
        if version >= CURRENT_VERSION {
            return Ok(false);
        }
        let before = version;
        for step in &self.steps {
            let current = get_version(json);
            if current == step.source_version() {
                step.migrate_json(json)?;
                json["version"] = Value::Number((step.source_version() + 1).into());
            }
        }
        Ok(get_version(json) != before)
    }
}

/// 从 JSON 值中读取 version 字段，缺省返回 1
fn get_version(json: &Value) -> u32 {
    json.get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32
}

/// 构建包含所有已知迁移步骤的注册表
pub fn default_registry() -> MigrationRegistry {
    MigrationRegistry::new()
}

/// 便利函数：迁移 JSON 字节，返回（是否发生迁移, 新版 JSON 字节）
pub fn migrate_manifest_bytes(manifest_bytes: &[u8]) -> Result<(bool, Option<Vec<u8>>), DiaryError> {
    let mut json: Value = serde_json::from_slice(manifest_bytes)?;
    let registry = default_registry();
    if registry.migrate(&mut json)? {
        let new_bytes = serde_json::to_vec(&json)?;
        Ok((true, Some(new_bytes)))
    } else {
        Ok((false, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_version_missing_field() {
        let json = serde_json::json!({"id": "test"});
        assert_eq!(get_version(&json), 1);
    }

    #[test]
    fn test_get_version_explicit() {
        let json = serde_json::json!({"id": "test", "version": 2});
        assert_eq!(get_version(&json), 2);
    }

    #[test]
    fn test_no_migration_needed() {
        let json_bytes = br#"{"id":"test","version":1,"content":"hello"}"#;
        let (migrated, new_bytes) = migrate_manifest_bytes(json_bytes).unwrap();
        assert!(!migrated);
        assert!(new_bytes.is_none());
    }

    #[test]
    fn test_missing_version_no_migration_when_current_is_1() {
        let json_bytes = br#"{"id":"test","content":"hello"}"#;
        let (migrated, _) = migrate_manifest_bytes(json_bytes).unwrap();
        assert!(!migrated);
    }

    #[test]
    fn test_version_received_greater_than_current() {
        let mut json = serde_json::json!({"id": "test", "content": "hello"});
        json["version"] = Value::Number(2.into());
        let bytes = serde_json::to_vec(&json).unwrap();
        let (migrated, _) = migrate_manifest_bytes(&bytes).unwrap();
        assert!(!migrated);
    }

    #[test]
    fn test_migration_registry_empty_noop() {
        let registry = default_registry();
        let mut json = serde_json::json!({"id": "test", "version": 0});
        let result = registry.migrate(&mut json).unwrap();
        assert!(result);
        // version clamped to CURRENT_VERSION even without migration steps
        assert_eq!(get_version(&json), 1);
    }
}

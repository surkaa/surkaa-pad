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

    #[allow(dead_code)]
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
pub fn get_version(json: &Value) -> u32 {
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

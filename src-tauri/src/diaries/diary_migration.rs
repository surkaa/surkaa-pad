use crate::diaries::DiaryError;
use serde_json::Value;
use tauri_plugin_log::log;

/// 代码当前支持的 schema 版本
pub const CURRENT_VERSION: u32 = 2;

/// 单个迁移步骤：将 JSON 从 V(n) 转换为 V(n+1)
pub trait DiaryMigration: Send + Sync {
    /// 此迁移步骤处理的源版本
    fn source_version(&self) -> u32;
    /// 在 JSON 值上执行迁移（就地修改）
    fn migrate_json(&self, json: &mut Value) -> Result<(), DiaryError>;

    /// [测试辅助] 构造源版本的测试输入 JSON
    #[cfg(test)]
    fn test_input(&self) -> Value {
        let mut json = Value::Object(serde_json::Map::new());
        json["version"] = Value::Number(self.source_version().into());
        json
    }
    /// [测试辅助] 验证迁移后的 JSON 是否符合预期，失败时 panic
    #[cfg(test)]
    fn test_verify(&self, _json: &Value) {}
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

    /// 返回已注册的迁移步骤，按版本顺序排列
    #[allow(dead_code)]
    pub fn steps(&self) -> &[Box<dyn DiaryMigration>] {
        &self.steps
    }

    /// 按序执行所有适用的迁移步骤，返回是否发生了迁移
    pub fn migrate(&self, json: &mut Value) -> Result<bool, DiaryError> {
        let version = get_version(json);
        if version >= CURRENT_VERSION {
            log::debug!("Manifest version {version} already current (latest {CURRENT_VERSION}), skip migration");
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
                step.migrate_json(json)?;
                json["version"] = Value::Number((step.source_version() + 1).into());
            }
        }
        let after = get_version(json);
        if after != before {
            log::info!("Manifest migration complete: V{before} → V{after}");
        }
        Ok(after != before)
    }
}

/// 从 JSON 值中读取 version 字段，缺省返回 1
pub fn get_version(json: &Value) -> u32 {
    json.get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32
}

/// V1 → V2: 为每个附件添加 etag 字段
struct V1ToV2Migration;

impl DiaryMigration for V1ToV2Migration {
    fn source_version(&self) -> u32 {
        1
    }
    fn migrate_json(&self, json: &mut Value) -> Result<(), DiaryError> {
        let mut count = 0;
        if let Some(attachments) = json.get_mut("attachments").and_then(|v| v.as_array_mut()) {
            for att in attachments.iter_mut() {
                if att.get("etag").is_none() {
                    att["etag"] = Value::Null;
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
        assert_eq!(get_version(json), 2);
        let attachments = json["attachments"].as_array().expect("attachments should be array");
        assert_eq!(attachments.len(), 2);
        for att in attachments {
            assert_eq!(att["etag"], Value::Null, "每个附件应有 etag: null");
        }
    }
}

/// 构建包含所有已知迁移步骤的注册表
pub fn default_registry() -> MigrationRegistry {
    let mut reg = MigrationRegistry::new();
    reg.register(Box::new(V1ToV2Migration));
    reg
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

use crate::object::{ObjectError, OssClient};
use crate::object_locations::{LegacyObjectLocations, ObjectLocations, StoredObject};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutObjectEntry {
    pub key: String,
    pub size: u64,
    pub etag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutObjectMove {
    pub source_key: String,
    pub target_key: String,
    pub size: u64,
    pub etag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutTargetConflict {
    pub source: LayoutObjectMove,
    pub target_size: u64,
    pub target_etag: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutMigrationPlan {
    pub pending: Vec<LayoutObjectMove>,
    pub already_copied: Vec<LayoutObjectMove>,
    pub conflicts: Vec<LayoutTargetConflict>,
    pub malformed_legacy_keys: Vec<String>,
    pub current_objects: Vec<LayoutObjectEntry>,
    pub unrelated: Vec<LayoutObjectEntry>,
}

impl LayoutMigrationPlan {
    pub fn legacy_object_count(&self) -> usize {
        self.pending.len() + self.already_copied.len() + self.conflicts.len()
    }

    pub fn legacy_bytes(&self) -> u64 {
        self.pending
            .iter()
            .chain(&self.already_copied)
            .map(|item| item.size)
            .chain(self.conflicts.iter().map(|item| item.source.size))
            .fold(0, u64::saturating_add)
    }

    pub fn pending_bytes(&self) -> u64 {
        self.pending
            .iter()
            .map(|item| item.size)
            .fold(0, u64::saturating_add)
    }

    pub fn is_safe_to_copy(&self) -> bool {
        self.conflicts.is_empty() && self.malformed_legacy_keys.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCopyResult {
    pub copied: usize,
    pub already_copied: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCleanupResult {
    pub deleted: usize,
}

#[derive(Debug, Error)]
pub enum LayoutMigrationError {
    #[error("对象存储操作失败: {0}")]
    Object(#[from] ObjectError),
    #[error("迁移计划存在 {conflicts} 个冲突、{malformed} 个无法识别的旧目录对象")]
    UnsafePlan { conflicts: usize, malformed: usize },
    #[error("目标对象在计划后出现，已停止且不会覆盖: {0}")]
    TargetAppeared(String),
    #[error("源对象不存在或在计划后发生变化: {0}")]
    SourceChanged(String),
    #[error("对象复制后不存在或与源对象不一致: {0}")]
    CopyVerificationFailed(String),
    #[error("仍有 {0} 个对象未复制，拒绝删除旧对象")]
    PendingObjects(usize),
    #[error("目标对象不存在或与源对象不一致，拒绝删除: {0}")]
    CleanupVerificationFailed(String),
    #[error("全量复查失败: remaining={remaining}, verified={verified}, expected={expected}")]
    FinalVerificationFailed {
        remaining: usize,
        verified: usize,
        expected: usize,
    },
    #[error("清理后仍存在 {0} 个旧布局对象")]
    CleanupIncomplete(usize),
}

pub async fn load_layout_migration_plan(
    client: &OssClient,
) -> Result<LayoutMigrationPlan, LayoutMigrationError> {
    let mut entries = Vec::new();
    let mut next_token = None;
    loop {
        let (page, token) = client.list("", next_token).await?;
        entries.extend(page.into_iter().map(|object| LayoutObjectEntry {
            key: object.key,
            size: object.size,
            etag: object.etag,
        }));
        let Some(token) = token else {
            break;
        };
        next_token = Some(token);
    }
    Ok(build_layout_migration_plan(entries))
}

pub async fn copy_layout_objects<F>(
    client: &OssClient,
    progress: F,
) -> Result<LayoutCopyResult, LayoutMigrationError>
where
    F: Fn(usize, usize, &LayoutObjectMove),
{
    let plan = load_layout_migration_plan(client).await?;
    ensure_plan_safe(&plan)?;

    for (index, movement) in plan.pending.iter().enumerate() {
        ensure_source_unchanged(client, movement).await?;
        if client.object_exists(&movement.target_key).await? {
            return Err(LayoutMigrationError::TargetAppeared(
                movement.target_key.clone(),
            ));
        }
        client
            .copy_object(&movement.source_key, &movement.target_key)
            .await?;
        let target = current_entry(client, &movement.target_key).await?;
        if target
            .as_ref()
            .is_none_or(|target| !movement_matches_entry(movement, target))
        {
            return Err(LayoutMigrationError::CopyVerificationFailed(
                movement.target_key.clone(),
            ));
        }
        progress(index + 1, plan.pending.len(), movement);
    }

    let verified = load_layout_migration_plan(client).await?;
    ensure_plan_safe(&verified)?;
    let expected = plan.legacy_object_count();
    if !verified.pending.is_empty() || verified.already_copied.len() != expected {
        return Err(LayoutMigrationError::FinalVerificationFailed {
            remaining: verified.pending.len(),
            verified: verified.already_copied.len(),
            expected,
        });
    }
    Ok(LayoutCopyResult {
        copied: plan.pending.len(),
        already_copied: plan.already_copied.len(),
    })
}

pub async fn cleanup_legacy_layout_objects<F>(
    client: &OssClient,
    progress: F,
) -> Result<LayoutCleanupResult, LayoutMigrationError>
where
    F: Fn(usize, usize, &LayoutObjectMove),
{
    let mut plan = load_layout_migration_plan(client).await?;
    ensure_plan_safe(&plan)?;
    if !plan.pending.is_empty() {
        return Err(LayoutMigrationError::PendingObjects(plan.pending.len()));
    }
    // 即便清理中途失败，也尽量最后再删除旧 manifest 提交标志。
    plan.already_copied.sort_by_key(|movement| {
        matches!(
            LegacyObjectLocations::parse(&movement.source_key),
            Some(StoredObject::DiaryManifest { .. })
        )
    });

    for (index, movement) in plan.already_copied.iter().enumerate() {
        ensure_source_unchanged(client, movement).await?;
        let target = current_entry(client, &movement.target_key).await?;
        if target
            .as_ref()
            .is_none_or(|target| !movement_matches_entry(movement, target))
        {
            return Err(LayoutMigrationError::CleanupVerificationFailed(
                movement.source_key.clone(),
            ));
        }
        client.delete(&movement.source_key).await?;
        progress(index + 1, plan.already_copied.len(), movement);
    }

    let verified = load_layout_migration_plan(client).await?;
    ensure_plan_safe(&verified)?;
    if verified.legacy_object_count() != 0 {
        return Err(LayoutMigrationError::CleanupIncomplete(
            verified.legacy_object_count(),
        ));
    }
    Ok(LayoutCleanupResult {
        deleted: plan.already_copied.len(),
    })
}

fn ensure_plan_safe(plan: &LayoutMigrationPlan) -> Result<(), LayoutMigrationError> {
    if plan.is_safe_to_copy() {
        Ok(())
    } else {
        Err(LayoutMigrationError::UnsafePlan {
            conflicts: plan.conflicts.len(),
            malformed: plan.malformed_legacy_keys.len(),
        })
    }
}

async fn ensure_source_unchanged(
    client: &OssClient,
    movement: &LayoutObjectMove,
) -> Result<(), LayoutMigrationError> {
    let source = current_entry(client, &movement.source_key).await?;
    if source
        .as_ref()
        .is_some_and(|source| movement_matches_entry(movement, source))
    {
        Ok(())
    } else {
        Err(LayoutMigrationError::SourceChanged(
            movement.source_key.clone(),
        ))
    }
}

async fn current_entry(
    client: &OssClient,
    key: &str,
) -> Result<Option<LayoutObjectEntry>, LayoutMigrationError> {
    if !client.object_exists(key).await? {
        return Ok(None);
    }
    let metadata = client.get_metadata(key).await?;
    Ok(Some(LayoutObjectEntry {
        key: key.to_string(),
        size: metadata.content_length.ok_or_else(|| {
            LayoutMigrationError::CopyVerificationFailed(format!("对象缺少 Content-Length: {key}"))
        })?,
        etag: metadata.etag,
    }))
}

fn movement_matches_entry(movement: &LayoutObjectMove, entry: &LayoutObjectEntry) -> bool {
    movement.size == entry.size
        && match (movement.etag.as_deref(), entry.etag.as_deref()) {
            (Some(left), Some(right)) => {
                normalize_etag(left).eq_ignore_ascii_case(normalize_etag(right))
            }
            _ => false,
        }
}

pub fn build_layout_migration_plan(
    entries: impl IntoIterator<Item = LayoutObjectEntry>,
) -> LayoutMigrationPlan {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    let by_key = entries
        .iter()
        .map(|entry| (entry.key.clone(), (entry.size, entry.etag.clone())))
        .collect::<HashMap<_, _>>();
    let mut plan = LayoutMigrationPlan::default();

    for entry in entries {
        if ObjectLocations::parse(&entry.key).is_some() {
            plan.current_objects.push(entry);
            continue;
        }

        let Some(object) = LegacyObjectLocations::parse(&entry.key) else {
            if starts_with_numeric_directory(&entry.key) {
                plan.malformed_legacy_keys.push(entry.key);
            } else {
                plan.unrelated.push(entry);
            }
            continue;
        };

        let target_key = ObjectLocations::key(&object);
        let movement = LayoutObjectMove {
            source_key: entry.key,
            target_key: target_key.clone(),
            size: entry.size,
            etag: entry.etag,
        };
        match by_key.get(&target_key) {
            None => plan.pending.push(movement),
            Some((target_size, target_etag))
                if objects_match(&movement, *target_size, target_etag.as_deref()) =>
            {
                plan.already_copied.push(movement);
            }
            Some((target_size, target_etag)) => plan.conflicts.push(LayoutTargetConflict {
                source: movement,
                target_size: *target_size,
                target_etag: target_etag.clone(),
            }),
        }
    }

    plan
}

fn starts_with_numeric_directory(key: &str) -> bool {
    key.split_once('/').is_some_and(|(first, _)| {
        !first.is_empty() && first.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn objects_match(source: &LayoutObjectMove, target_size: u64, target_etag: Option<&str>) -> bool {
    source.size == target_size
        && match (source.etag.as_deref(), target_etag) {
            (Some(source), Some(target)) => {
                normalize_etag(source).eq_ignore_ascii_case(normalize_etag(target))
            }
            _ => false,
        }
}

fn normalize_etag(etag: &str) -> &str {
    etag.trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestOssGuard;

    fn entry(key: &str, size: u64, etag: Option<&str>) -> LayoutObjectEntry {
        LayoutObjectEntry {
            key: key.into(),
            size,
            etag: etag.map(str::to_string),
        }
    }

    #[test]
    fn plans_legacy_manifests_attachments_and_transaction_backups() {
        let plan = build_layout_migration_plan([
            entry("123/manifest.enc", 10, Some("manifest")),
            entry("123/att-1", 20, Some("attachment")),
            entry("123/.attachment-transaction/att-2", 30, Some("backup")),
        ]);

        assert!(plan.is_safe_to_copy());
        assert_eq!(plan.legacy_object_count(), 3);
        assert_eq!(plan.legacy_bytes(), 60);
        assert_eq!(plan.pending_bytes(), 60);
        assert_eq!(
            plan.pending
                .iter()
                .map(|item| item.target_key.as_str())
                .collect::<Vec<_>>(),
            [
                "diaries/123/.attachment-transaction/att-2",
                "diaries/123/attachments/att-1",
                "diaries/123/manifest.enc",
            ]
        );
    }

    #[test]
    fn recognizes_idempotently_copied_targets_by_size_and_etag() {
        let plan = build_layout_migration_plan([
            entry("123/att-1", 20, Some("\"ABC\"")),
            entry("diaries/123/attachments/att-1", 20, Some("abc")),
        ]);

        assert!(plan.pending.is_empty());
        assert_eq!(plan.already_copied.len(), 1);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn blocks_copy_when_existing_target_does_not_match() {
        let plan = build_layout_migration_plan([
            entry("123/att-1", 20, Some("source")),
            entry("diaries/123/attachments/att-1", 21, Some("target")),
        ]);

        assert!(!plan.is_safe_to_copy());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.pending.is_empty());
    }

    #[test]
    fn separates_unrelated_namespaces_from_malformed_legacy_keys() {
        let plan = build_layout_migration_plan([
            entry("ai/sessions/1/meta.enc", 1, Some("ai")),
            entry("rust-tests/run/object", 2, Some("test")),
            entry("123/nested/unknown", 3, Some("unknown")),
            entry("not-a-diary/manifest.enc", 4, Some("other")),
        ]);

        assert_eq!(plan.malformed_legacy_keys, ["123/nested/unknown"]);
        assert_eq!(plan.unrelated.len(), 3);
        assert!(!plan.is_safe_to_copy());
    }

    #[test]
    fn ignores_objects_already_in_current_layout() {
        let plan = build_layout_migration_plan([
            entry("diaries/123/manifest.enc", 10, Some("manifest")),
            entry("diaries/123/attachments/att-1", 20, Some("attachment")),
        ]);

        assert_eq!(plan.current_objects.len(), 2);
        assert!(plan.pending.is_empty());
        assert!(plan.already_copied.is_empty());
        assert!(plan.conflicts.is_empty());
        assert!(plan.unrelated.is_empty());
    }

    #[tokio::test]
    async fn copy_verify_and_cleanup_are_idempotent_and_leave_unrelated_objects_untouched() {
        let client = OssClient::from_env();
        let (client, guard) = TestOssGuard::new(client).await;
        let legacy_objects = [
            ("123/manifest.enc", b"manifest".as_slice()),
            ("123/att-1", b"attachment-one".as_slice()),
            ("123/att-2", b"attachment-two".as_slice()),
        ];
        for (key, data) in legacy_objects {
            client.upload_bytes(key, data).await.unwrap();
        }
        let unrelated_key = "ai/sessions/1/meta.enc";
        client
            .upload_bytes(unrelated_key, b"unrelated")
            .await
            .unwrap();

        assert!(matches!(
            cleanup_legacy_layout_objects(&client, |_, _, _| {}).await,
            Err(LayoutMigrationError::PendingObjects(3))
        ));
        for (key, _) in legacy_objects {
            assert!(client.object_exists(key).await.unwrap());
        }

        let copied = copy_layout_objects(&client, |_, _, _| {}).await.unwrap();
        assert_eq!(
            copied,
            LayoutCopyResult {
                copied: 3,
                already_copied: 0,
            }
        );
        let repeated = copy_layout_objects(&client, |_, _, _| {}).await.unwrap();
        assert_eq!(
            repeated,
            LayoutCopyResult {
                copied: 0,
                already_copied: 3,
            }
        );
        for (source, data) in legacy_objects {
            let object = LegacyObjectLocations::parse(source).unwrap();
            assert_eq!(
                client
                    .download_bytes(&ObjectLocations::key(&object))
                    .await
                    .unwrap(),
                data
            );
        }

        let cleaned = cleanup_legacy_layout_objects(&client, |_, _, _| {})
            .await
            .unwrap();
        assert_eq!(cleaned, LayoutCleanupResult { deleted: 3 });
        let repeated_cleanup = cleanup_legacy_layout_objects(&client, |_, _, _| {})
            .await
            .unwrap();
        assert_eq!(repeated_cleanup, LayoutCleanupResult { deleted: 0 });
        for (key, _) in legacy_objects {
            assert!(!client.object_exists(key).await.unwrap());
        }
        assert_eq!(
            client.download_bytes(unrelated_key).await.unwrap(),
            b"unrelated"
        );
        guard.cleanup().await;
    }
}

use crate::object::{ObjectError, OssClient};
use crate::object_locations::{LegacyObjectLocations, ObjectLocations, StoredObject};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use thiserror::Error;

const LOCAL_DATA_SUFFIX: &str = ".data";
const LOCAL_ETAG_SUFFIX: &str = ".md5";

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalLayoutMigrationResult {
    pub migrated: usize,
    pub recovered: usize,
    pub deduplicated: usize,
}

#[derive(Debug, Error)]
pub enum LocalLayoutMigrationError {
    #[error("本地对象布局迁移 I/O 失败: path={path}, error={source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("新旧本地对象内容冲突: {source_key} -> {target_key}")]
    Conflict {
        source_key: String,
        target_key: String,
    },
    #[error("发现不完整的旧本地对象，无法自动迁移: {0}")]
    IncompleteLegacyObject(String),
    #[error("本地对象路径不是有效 UTF-8: {0}")]
    InvalidPath(PathBuf),
}

/// 将同一个 LOS 目录中的旧日记 Key 一次性移动到当前固定布局。
///
/// 只使用同文件系统内的重命名，不复制对象内容；重复执行是幂等的，并能恢复
/// `.data` 已移动但 `.md5` 尚未移动（或相反）的中断状态。
pub fn migrate_legacy_local_object_layout(
    root: &Path,
) -> Result<LocalLayoutMigrationResult, LocalLayoutMigrationError> {
    let keys = collect_legacy_local_keys(root)?;
    let mut result = LocalLayoutMigrationResult::default();
    for source_key in keys {
        let object = LegacyObjectLocations::parse(&source_key)
            .expect("collect_legacy_local_keys 只返回可解析的旧 Key");
        let target_key = ObjectLocations::key(&object);
        match migrate_local_object(root, &source_key, &target_key)? {
            LocalObjectMoveOutcome::Migrated => result.migrated += 1,
            LocalObjectMoveOutcome::Recovered => result.recovered += 1,
            LocalObjectMoveOutcome::Deduplicated => result.deduplicated += 1,
        }
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalObjectMoveOutcome {
    Migrated,
    Recovered,
    Deduplicated,
}

fn migrate_local_object(
    root: &Path,
    source_key: &str,
    target_key: &str,
) -> Result<LocalObjectMoveOutcome, LocalLayoutMigrationError> {
    let (source_data, source_etag) = local_object_paths(root, source_key);
    let (target_data, target_etag) = local_object_paths(root, target_key);
    let source_state = (source_data.exists(), source_etag.exists());
    let target_state = (target_data.exists(), target_etag.exists());

    match (source_state, target_state) {
        ((true, true), (false, false)) => {
            create_parent(&target_data)?;
            rename(&source_data, &target_data)?;
            rename(&source_etag, &target_etag)?;
            remove_empty_source_directories(&source_data, root);
            Ok(LocalObjectMoveOutcome::Migrated)
        }
        ((false, true), (true, false)) => {
            create_parent(&target_etag)?;
            rename(&source_etag, &target_etag)?;
            remove_empty_source_directories(&source_data, root);
            Ok(LocalObjectMoveOutcome::Recovered)
        }
        ((true, false), (false, true)) => {
            create_parent(&target_data)?;
            rename(&source_data, &target_data)?;
            remove_empty_source_directories(&source_data, root);
            Ok(LocalObjectMoveOutcome::Recovered)
        }
        ((true, true), (true, true)) => {
            if !local_objects_match(&source_data, &source_etag, &target_data, &target_etag)? {
                return Err(LocalLayoutMigrationError::Conflict {
                    source_key: source_key.to_string(),
                    target_key: target_key.to_string(),
                });
            }
            remove_file(&source_data)?;
            remove_file(&source_etag)?;
            remove_empty_source_directories(&source_data, root);
            Ok(LocalObjectMoveOutcome::Deduplicated)
        }
        ((false, true), (true, true)) => {
            if read_etag(&source_etag)?.eq_ignore_ascii_case(&read_etag(&target_etag)?) {
                remove_file(&source_etag)?;
                remove_empty_source_directories(&source_data, root);
                Ok(LocalObjectMoveOutcome::Recovered)
            } else {
                Err(LocalLayoutMigrationError::Conflict {
                    source_key: source_key.to_string(),
                    target_key: target_key.to_string(),
                })
            }
        }
        ((true, false), (true, true)) => {
            if file_size(&source_data)? == file_size(&target_data)? {
                remove_file(&source_data)?;
                remove_empty_source_directories(&source_data, root);
                Ok(LocalObjectMoveOutcome::Recovered)
            } else {
                Err(LocalLayoutMigrationError::Conflict {
                    source_key: source_key.to_string(),
                    target_key: target_key.to_string(),
                })
            }
        }
        ((false, false), _) => unreachable!("旧 Key 必须至少存在一个物理文件"),
        _ => Err(LocalLayoutMigrationError::IncompleteLegacyObject(
            source_key.to_string(),
        )),
    }
}

fn collect_legacy_local_keys(root: &Path) -> Result<BTreeSet<String>, LocalLayoutMigrationError> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    let mut keys = BTreeSet::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in
            std::fs::read_dir(&directory).map_err(|source| LocalLayoutMigrationError::Io {
                path: directory.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| LocalLayoutMigrationError::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|source| LocalLayoutMigrationError::Io {
                    path: path.clone(),
                    source,
                })?;
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("枚举路径必须位于 LOS 根目录");
            let relative = relative
                .components()
                .map(|component| component.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| LocalLayoutMigrationError::InvalidPath(path.clone()))?
                .join("/");
            let key = relative
                .strip_suffix(LOCAL_DATA_SUFFIX)
                .or_else(|| relative.strip_suffix(LOCAL_ETAG_SUFFIX));
            if let Some(key) = key.filter(|key| LegacyObjectLocations::parse(key).is_some()) {
                keys.insert(key.to_string());
            }
        }
    }
    Ok(keys)
}

fn local_object_paths(root: &Path, key: &str) -> (PathBuf, PathBuf) {
    (
        root.join(format!("{key}{LOCAL_DATA_SUFFIX}")),
        root.join(format!("{key}{LOCAL_ETAG_SUFFIX}")),
    )
}

fn local_objects_match(
    source_data: &Path,
    source_etag: &Path,
    target_data: &Path,
    target_etag: &Path,
) -> Result<bool, LocalLayoutMigrationError> {
    Ok(file_size(source_data)? == file_size(target_data)?
        && read_etag(source_etag)?.eq_ignore_ascii_case(&read_etag(target_etag)?))
}

fn file_size(path: &Path) -> Result<u64, LocalLayoutMigrationError> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|source| LocalLayoutMigrationError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn read_etag(path: &Path) -> Result<String, LocalLayoutMigrationError> {
    std::fs::read_to_string(path)
        .map(|etag| etag.trim().trim_matches('"').to_string())
        .map_err(|source| LocalLayoutMigrationError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn create_parent(path: &Path) -> Result<(), LocalLayoutMigrationError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| LocalLayoutMigrationError::Io {
        path: parent.to_path_buf(),
        source,
    })
}

fn rename(source_path: &Path, target_path: &Path) -> Result<(), LocalLayoutMigrationError> {
    std::fs::rename(source_path, target_path).map_err(|source| LocalLayoutMigrationError::Io {
        path: source_path.to_path_buf(),
        source,
    })
}

fn remove_file(path: &Path) -> Result<(), LocalLayoutMigrationError> {
    std::fs::remove_file(path).map_err(|source| LocalLayoutMigrationError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn remove_empty_source_directories(source_data: &Path, root: &Path) {
    let mut directory = source_data.parent();
    while let Some(path) = directory {
        if path == root || std::fs::remove_dir(path).is_err() {
            break;
        }
        directory = path.parent();
    }
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
    use crate::caches::LocalObjectStore;
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

    #[tokio::test]
    async fn local_layout_migration_moves_objects_without_touching_unrelated_files() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        let legacy_manifest = "123/manifest.enc";
        let legacy_attachment = "123/att-1";
        let unrelated = "ai/sessions/1/meta.enc";
        los.save_bytes(legacy_manifest, b"manifest").await.unwrap();
        los.save_bytes(legacy_attachment, b"attachment")
            .await
            .unwrap();
        los.save_bytes(unrelated, b"unrelated").await.unwrap();

        let result = migrate_legacy_local_object_layout(temp.path()).unwrap();

        assert_eq!(
            result,
            LocalLayoutMigrationResult {
                migrated: 2,
                recovered: 0,
                deduplicated: 0,
            }
        );
        for (source, expected) in [
            (legacy_manifest, b"manifest".as_slice()),
            (legacy_attachment, b"attachment".as_slice()),
        ] {
            assert!(los.get(source).await.unwrap().is_none());
            let target = ObjectLocations::key(&LegacyObjectLocations::parse(source).unwrap());
            assert_eq!(los.get_data(&target).await.unwrap(), expected);
        }
        assert_eq!(los.get_data(unrelated).await.unwrap(), b"unrelated");
        assert_eq!(
            migrate_legacy_local_object_layout(temp.path()).unwrap(),
            LocalLayoutMigrationResult::default()
        );
    }

    #[test]
    fn local_layout_migration_recovers_each_interrupted_rename_state() {
        let temp = tempfile::tempdir().unwrap();
        let first_source = "123/att-1";
        let first_target = ObjectLocations::diary_attachment("123", "att-1");
        let (first_source_data, first_source_etag) = local_object_paths(temp.path(), first_source);
        let (first_target_data, first_target_etag) = local_object_paths(temp.path(), &first_target);
        create_parent(&first_source_etag).unwrap();
        create_parent(&first_target_data).unwrap();
        std::fs::write(&first_source_etag, "etag-1").unwrap();
        std::fs::write(&first_target_data, b"data-1").unwrap();
        assert!(!first_source_data.exists());
        assert!(!first_target_etag.exists());

        let second_source = "456/att-2";
        let second_target = ObjectLocations::diary_attachment("456", "att-2");
        let (second_source_data, second_source_etag) =
            local_object_paths(temp.path(), second_source);
        let (second_target_data, second_target_etag) =
            local_object_paths(temp.path(), &second_target);
        create_parent(&second_source_data).unwrap();
        create_parent(&second_target_etag).unwrap();
        std::fs::write(&second_source_data, b"data-2").unwrap();
        std::fs::write(&second_target_etag, "etag-2").unwrap();
        assert!(!second_source_etag.exists());
        assert!(!second_target_data.exists());

        let result = migrate_legacy_local_object_layout(temp.path()).unwrap();

        assert_eq!(result.recovered, 2);
        assert_eq!(std::fs::read(first_target_data).unwrap(), b"data-1");
        assert_eq!(read_etag(&first_target_etag).unwrap(), "etag-1");
        assert_eq!(std::fs::read(second_target_data).unwrap(), b"data-2");
        assert_eq!(read_etag(&second_target_etag).unwrap(), "etag-2");
    }

    #[tokio::test]
    async fn local_layout_migration_deduplicates_equal_targets_and_rejects_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let los = LocalObjectStore::new(temp.path().to_path_buf());
        let equal_source = "123/att-equal";
        let equal_target = ObjectLocations::diary_attachment("123", "att-equal");
        los.save_bytes(equal_source, b"same").await.unwrap();
        los.save_bytes(&equal_target, b"same").await.unwrap();

        let result = migrate_legacy_local_object_layout(temp.path()).unwrap();
        assert_eq!(result.deduplicated, 1);
        assert!(los.get(equal_source).await.unwrap().is_none());
        assert_eq!(los.get_data(&equal_target).await.unwrap(), b"same");

        let conflict_source = "456/att-conflict";
        let conflict_target = ObjectLocations::diary_attachment("456", "att-conflict");
        los.save_bytes(conflict_source, b"source").await.unwrap();
        los.save_bytes(&conflict_target, b"target").await.unwrap();

        assert!(matches!(
            migrate_legacy_local_object_layout(temp.path()),
            Err(LocalLayoutMigrationError::Conflict { .. })
        ));
        assert_eq!(los.get_data(conflict_source).await.unwrap(), b"source");
        assert_eq!(los.get_data(&conflict_target).await.unwrap(), b"target");
    }
}

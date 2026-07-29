use super::{display_path, LocalStorageMigrationError, LocalStorageMigrationPlan};
use crate::app_config::{LocalStorageLocation, PendingLocalStorageMigration};
use crate::caches::{LocalObjectEntry, LocalObjectStore};
use crate::local_storage::{
    available_space_for, existing_ancestor, required_space_with_margin, LocalStorageManager,
};
#[cfg(windows)]
use std::path::Component;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(super) struct MigrationRequest {
    pub(super) location: LocalStorageLocation,
    pub(super) target_root: PathBuf,
}

pub(super) struct MigrationPlanInternal {
    pub(super) request: MigrationRequest,
    pub(super) source_root: PathBuf,
    pub(super) entries: Vec<LocalObjectEntry>,
    pub(super) available_bytes: u64,
    pub(super) required_bytes: u64,
    pub(super) fast_move: bool,
}

impl MigrationPlanInternal {
    pub(super) fn total_bytes(&self) -> u64 {
        self.entries
            .iter()
            .fold(0u64, |total, entry| total.saturating_add(entry.size))
    }

    pub(super) fn public(&self) -> LocalStorageMigrationPlan {
        LocalStorageMigrationPlan {
            source_path: display_path(&self.source_root),
            target_path: display_path(&self.request.target_root),
            total_files: self.entries.len() as u32,
            total_bytes: self.total_bytes(),
            available_bytes: self.available_bytes,
            required_bytes: self.required_bytes,
            fast_move: self.fast_move,
        }
    }
}

pub(super) async fn build_plan(
    source: &LocalObjectStore,
    request: MigrationRequest,
) -> Result<MigrationPlanInternal, LocalStorageMigrationError> {
    build_plan_for_resume(source, request, None).await
}

pub(super) async fn build_plan_for_resume(
    source: &LocalObjectStore,
    request: MigrationRequest,
    pending: Option<&PendingLocalStorageMigration>,
) -> Result<MigrationPlanInternal, LocalStorageMigrationError> {
    let source_root = source.root().to_path_buf();
    validate_path_relationship(&source_root, &request.target_root)?;
    let entries = source.get_all_entries().await?;
    if source_root == request.target_root {
        return Ok(MigrationPlanInternal {
            request,
            source_root,
            entries,
            available_bytes: available_space_for(source.root())?,
            required_bytes: 0,
            fast_move: true,
        });
    }
    let target_nonempty =
        request.target_root.exists() && directory_has_entries(&request.target_root)?;
    let resuming_completed_target = pending.is_some() && target_nonempty;
    if target_nonempty && !resuming_completed_target {
        return Err(LocalStorageMigrationError::TargetNotEmpty(display_path(
            &request.target_root,
        )));
    }

    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let available_bytes = available_space_for(&request.target_root)?;
    let fast_move = source_root.exists()
        && !target_nonempty
        && same_filesystem(&source_root, &request.target_root)?;
    let staging_bytes = pending
        .map(|pending| directory_size(pending.staging_root()))
        .transpose()?
        .unwrap_or(0);
    let required_bytes = if fast_move || resuming_completed_target {
        0
    } else {
        required_space_with_margin(total_bytes).saturating_sub(staging_bytes)
    };

    Ok(MigrationPlanInternal {
        request,
        source_root,
        entries,
        available_bytes,
        required_bytes,
        fast_move,
    })
}

pub(super) fn resolve_request(
    manager: &LocalStorageManager,
    base_path: Option<String>,
) -> Result<MigrationRequest, LocalStorageMigrationError> {
    let location = match base_path {
        None => LocalStorageLocation::Default,
        Some(path) => {
            if !cfg!(target_os = "windows") && !cfg!(test) {
                return Err(LocalStorageMigrationError::UnsupportedPlatform);
            }
            let base_path = PathBuf::from(path);
            if !base_path.is_absolute() {
                return Err(LocalStorageMigrationError::RelativePath);
            }
            if !base_path.is_dir() {
                return Err(LocalStorageMigrationError::InvalidBasePath(display_path(
                    &base_path,
                )));
            }
            LocalStorageLocation::Custom {
                base_path: dunce::simplified(&base_path.canonicalize()?).to_path_buf(),
            }
        }
    };
    Ok(MigrationRequest {
        target_root: manager.root_for_location(&location),
        location,
    })
}

pub(super) fn validate_path_relationship(
    source: &Path,
    target: &Path,
) -> Result<(), LocalStorageMigrationError> {
    if source == target {
        return Ok(());
    }
    if target.starts_with(source) || source.starts_with(target) {
        return Err(LocalStorageMigrationError::OverlappingPath);
    }
    Ok(())
}

pub(super) fn directory_has_entries(path: &Path) -> Result<bool, std::io::Error> {
    match std::fs::read_dir(path) {
        Ok(mut entries) => Ok(entries.next().transpose()?.is_some()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn directory_size(path: &Path) -> Result<u64, std::io::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

pub(super) fn staging_root(target_root: &Path) -> PathBuf {
    let name = target_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("los");
    target_root.with_file_name(format!("{name}.migrating"))
}

#[cfg(windows)]
fn same_filesystem(source: &Path, target: &Path) -> Result<bool, std::io::Error> {
    fn prefix(path: &Path) -> Option<String> {
        path.components().find_map(|component| match component {
            Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_ascii_lowercase())
            }
            _ => None,
        })
    }
    let source = source.canonicalize()?;
    let target = existing_ancestor(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "目标磁盘不存在"))?
        .canonicalize()?;
    Ok(prefix(&source).is_some() && prefix(&source) == prefix(&target))
}

#[cfg(unix)]
fn same_filesystem(source: &Path, target: &Path) -> Result<bool, std::io::Error> {
    use std::os::unix::fs::MetadataExt;
    let target = existing_ancestor(target)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "目标磁盘不存在"))?;
    Ok(std::fs::metadata(source)?.dev() == std::fs::metadata(target)?.dev())
}

#[cfg(not(any(windows, unix)))]
fn same_filesystem(_source: &Path, _target: &Path) -> Result<bool, std::io::Error> {
    Ok(false)
}

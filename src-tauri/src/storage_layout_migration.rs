use crate::object_locations::{LegacyObjectLocations, ObjectLocations};
use std::collections::HashMap;

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

        assert_eq!(plan, LayoutMigrationPlan::default());
    }
}

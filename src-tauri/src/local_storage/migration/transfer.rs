use super::{send_progress, LocalStorageMigrationError, LocalStorageMigrationEvent};
use crate::caches::{LocalObjectEntry, LocalObjectStore};
use crate::local_storage::migration::LocalStorageMigrationPhase;
use crate::utils::message_sender::MessageSender;
use futures_util::StreamExt;
use std::sync::Arc;

pub(super) async fn copy_entries(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entries: &[LocalObjectEntry],
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<(), LocalStorageMigrationError> {
    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let mut completed_bytes = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        if object_matches(source, target, entry, &event).await? {
            completed_bytes = completed_bytes.saturating_add(entry.size);
            send_progress(
                &event,
                LocalStorageMigrationPhase::Copying,
                entry,
                index,
                entries.len(),
                entry.size,
                completed_bytes.saturating_sub(entry.size),
                total_bytes,
            );
            continue;
        }
        if target.get(&entry.key).await?.is_some() {
            target.delete(&entry.key).await?;
        }
        let mut stream = source.get_stream(&entry.key, None).await?;
        let handle = target.begin_chunked_save(&entry.key).await?;
        let mut current_bytes = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| crate::caches::CacheError::StreamError)?;
            if let Err(error) = handle.write_chunk(&chunk).await {
                handle.abort().await;
                return Err(error.into());
            }
            current_bytes = current_bytes.saturating_add(chunk.len() as u64);
            send_progress(
                &event,
                LocalStorageMigrationPhase::Copying,
                entry,
                index,
                entries.len(),
                current_bytes,
                completed_bytes,
                total_bytes,
            );
        }
        handle.finalize(&entry.etag).await?;
        completed_bytes = completed_bytes.saturating_add(entry.size);
    }
    Ok(())
}

async fn object_matches(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entry: &LocalObjectEntry,
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<bool, LocalStorageMigrationError> {
    if target.get(&entry.key).await?.as_deref() != Some(entry.etag.as_str())
        || target.get_size(&entry.key).await? != Some(entry.size)
    {
        return Ok(false);
    }
    let source_hash = hash_object(source, entry, None, 0, 0, 0, event).await?;
    let target_hash = hash_object(target, entry, None, 0, 0, 0, event).await?;
    Ok(source_hash == target_hash)
}

pub(super) async fn verify_entries(
    source: &LocalObjectStore,
    target: &LocalObjectStore,
    entries: &[LocalObjectEntry],
    event: Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<(), LocalStorageMigrationError> {
    let total_bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let mut completed_bytes = 0u64;
    for (index, entry) in entries.iter().enumerate() {
        let target_etag = target.get(&entry.key).await?;
        let target_size = target.get_size(&entry.key).await?;
        if target_etag.as_deref() != Some(entry.etag.as_str()) || target_size != Some(entry.size) {
            return Err(LocalStorageMigrationError::VerificationFailed {
                key: entry.key.clone(),
            });
        }

        let source_hash = hash_object(source, entry, None, entries.len(), 0, 0, &event).await?;
        let target_hash = hash_object(
            target,
            entry,
            Some(index),
            entries.len(),
            completed_bytes,
            total_bytes,
            &event,
        )
        .await?;
        if source_hash != target_hash {
            return Err(LocalStorageMigrationError::VerificationFailed {
                key: entry.key.clone(),
            });
        }
        completed_bytes = completed_bytes.saturating_add(entry.size);
    }
    Ok(())
}

async fn hash_object(
    store: &LocalObjectStore,
    entry: &LocalObjectEntry,
    progress_index: Option<usize>,
    total_files: usize,
    completed_bytes: u64,
    total_bytes: u64,
    event: &Arc<dyn MessageSender<LocalStorageMigrationEvent>>,
) -> Result<md5::Digest, LocalStorageMigrationError> {
    let mut stream = store.get_stream(&entry.key, None).await?;
    let mut context = md5::Context::new();
    let mut current_bytes = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| crate::caches::CacheError::StreamError)?;
        context.consume(&chunk);
        current_bytes = current_bytes.saturating_add(chunk.len() as u64);
        if let Some(index) = progress_index {
            send_progress(
                event,
                LocalStorageMigrationPhase::Verifying,
                entry,
                index,
                total_files,
                current_bytes,
                completed_bytes,
                total_bytes,
            );
        }
    }
    Ok(context.finalize())
}

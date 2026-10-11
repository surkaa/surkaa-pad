use super::engine::{progress, run};
use super::inventory::inventory;
use super::journal::MAX_JOURNAL_BYTES;
use super::storage::{read_limited, RekeyStorage, Storage};
use super::types::*;
use super::{error, Result};
use crate::cryptos::Crypto;
use crate::local_storage::{available_space_for, required_space_with_margin};
use crate::object_locations::ObjectLocations;
use crate::state::AppState;
use crate::vault_bootstrap::{KeyDerivationParameters, VAULT_VERIFIER_TEXT};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{ipc::Channel, State};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct KeyChangeManager {
    prepared: Mutex<Option<Prepared>>,
    paused: AtomicBool,
}

impl KeyChangeManager {
    pub(crate) fn request_pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }
}

struct Prepared {
    created: Instant,
    entries: Vec<Entry>,
    journal: Journal,
    checkpoint: Checkpoint,
    old: Crypto,
    new: Crypto,
}

fn storage(state: &AppState, remote: bool) -> Storage {
    Storage {
        state: state.clone(),
        remote,
    }
}

fn rotate_local_config(
    old: &Crypto,
    new: &Crypto,
    config: LocalEncryptedConfig,
) -> Result<LocalEncryptedConfig> {
    let rotate = |data: Option<Vec<u8>>| -> Result<Option<Vec<u8>>> {
        data.map(|data| {
            let plaintext = Zeroizing::new(old.decrypt(&data).map_err(error)?);
            new.encrypt(&plaintext).map_err(error)
        })
        .transpose()
    };
    Ok(LocalEncryptedConfig {
        vault_verifier: rotate(config.vault_verifier)?,
        oss: rotate(config.oss)?,
        ai: rotate(config.ai)?,
    })
}

/// 检查完整清单并实测派生与重加密；不修改任何用户数据。
#[tauri::command]
#[specta::specta]
pub async fn cmd_plan_vault_key_change(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
    confirm_password: String,
    memory_cost_kib: Option<u32>,
    local_config: LocalEncryptedConfig,
) -> Result<KeyChangePlan> {
    plan_change(
        &state,
        old_password,
        new_password,
        confirm_password,
        memory_cost_kib,
        local_config,
    )
    .await
}

pub(super) async fn plan_change(
    state: &AppState,
    old_password: String,
    new_password: String,
    confirm_password: String,
    memory_cost_kib: Option<u32>,
    local_config: LocalEncryptedConfig,
) -> Result<KeyChangePlan> {
    state.ensure_storage_available()?;
    let _guard = state
        .try_lock_key_change()
        .ok_or_else(|| error("请等待当前日记、附件或 AI 操作完成后再改密"))?;
    let old_password = Zeroizing::new(old_password);
    let new_password = Zeroizing::new(new_password);
    let confirm_password = Zeroizing::new(confirm_password);
    if new_password.is_empty() || *new_password != *confirm_password {
        return Err(error("新密码不能为空，且两次输入必须一致"));
    }
    let old_bootstrap = state
        .vault_bootstrap()
        .ok_or_else(|| error("请先建立密钥派生配置"))?;
    let old = state.crypto().snapshot().map_err(error)?;
    let start = Instant::now();
    if !old
        .validate_bootstrap_for_active_key(old_password.to_string(), &old_bootstrap)
        .map_err(error)?
    {
        return Err(error("旧密码与当前解锁的 Vault 不匹配"));
    }
    let mut params = old_bootstrap.kdf.clone();
    params.salt = KeyDerivationParameters::new_random(params.memory_cost_kib)
        .map_err(error)?
        .salt;
    if let Some(memory) = memory_cost_kib {
        params.memory_cost_kib = memory;
    }
    params.validate().map_err(error)?;
    let new = Crypto::new();
    new.derive_dek_with_parameters(new_password.to_string(), params.clone())
        .map_err(error)?;
    let derivation_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut new_bootstrap = old_bootstrap.clone();
    new_bootstrap.kdf = params;
    new_bootstrap.encrypted_verifier =
        STANDARD.encode(new.encrypt(VAULT_VERIFIER_TEXT.as_bytes()).map_err(error)?);
    // 新盐必须实际产生不同的密钥，极端碰撞也不能复用 CTR 的 key/nonce。
    if old
        .decrypt(&new_bootstrap.decode_verifier().map_err(error)?)
        .is_ok()
    {
        return Err(error("新密钥与旧密钥相同，请重新生成盐"));
    }
    let remote = state.is_remote_enabled();
    let store = storage(state, remote);
    if remote {
        check_cloud_start(&store, &old_bootstrap).await?;
        if local_config.oss.is_none() {
            return Err(error("缺少本机 OSS 加密配置，无法保证重启后续改"));
        }
    }
    let (entries, items) = inventory(&store, &old).await?;
    let local_config = rotate_local_config(&old, &new, local_config)?;
    let encrypted_bytes = items.iter().try_fold(0_u64, |sum, i| {
        sum.checked_add(i.source.size)
            .ok_or_else(|| error("总大小溢出"))
    })?;
    let total_bytes = entries.iter().try_fold(0_u64, |sum, e| {
        sum.checked_add(e.size).ok_or_else(|| error("总大小溢出"))
    })?;
    let temporary_bytes = items
        .iter()
        .map(|i| i.source.size.saturating_add(4096))
        .max()
        .unwrap_or(0)
        .saturating_mul(3);
    if !remote
        && available_space_for(state.local_object_store().root()).map_err(error)?
            < required_space_with_margin(temporary_bytes)
    {
        return Err(error("本地空间不足以安全改密，请先释放磁盘空间"));
    }
    let sample = Zeroizing::new(vec![0x5a; 1024 * 1024]);
    let start = Instant::now();
    for _ in 0..3 {
        let old_data = old.encrypt(&sample).map_err(error)?;
        let plain = Zeroizing::new(old.decrypt(&old_data).map_err(error)?);
        let new_data = new.encrypt(&plain).map_err(error)?;
        if new.decrypt(&new_data).map_err(error)? != *sample {
            return Err(error("重加密自检失败"));
        }
    }
    let crypto_estimate_ms = start.elapsed().as_secs_f64() * 1000.0 * encrypted_bytes as f64
        / (3.0 * sample.len() as f64);
    let (journal, checkpoint) = Journal::create(
        &old,
        &new,
        old_bootstrap,
        new_bootstrap.clone(),
        remote,
        items,
        local_config,
    )?;
    let plan = KeyChangePlan {
        plan_id: journal.transaction_id.clone(),
        remote,
        total_files: u32::try_from(entries.len()).map_err(error)?,
        total_bytes,
        encrypted_files: u32::try_from(checkpoint.items.len()).map_err(error)?,
        encrypted_bytes,
        temporary_bytes,
        derivation_ms,
        crypto_estimate_ms,
        // 最坏大对象路线：11 次读取 + 3 次写入（含备份/正式替换的流式复制）。
        // <= 1 GiB 使用 OSS 服务端复制，实际流量更小。这里只估算主要数据，
        // 不含检查点/请求开销和中断重试，不能当作严格的流量上限或网络耗时。
        transfer_bytes: if remote {
            encrypted_bytes.saturating_mul(14)
        } else {
            0
        },
        new_bootstrap,
    };
    *state.key_change_manager().prepared.lock().await = Some(Prepared {
        created: Instant::now(),
        entries,
        journal,
        checkpoint,
        old,
        new,
    });
    Ok(plan)
}

async fn check_cloud_start(
    store: &Storage,
    expected: &crate::vault_bootstrap::VaultBootstrap,
) -> Result<()> {
    store
        .state
        .oss_client()
        .require_unversioned_bucket()
        .await
        .map_err(error)?;
    if store
        .head(ObjectLocations::vault_key_change())
        .await?
        .is_some()
    {
        return Err(error("云端有未完成的改密，请先继续已有改密"));
    }
    let json = read_limited(store, ObjectLocations::vault_bootstrap(), 16 * 1024).await?;
    let bootstrap = crate::vault_bootstrap::VaultBootstrap::from_json(
        std::str::from_utf8(&json).map_err(error)?,
    )
    .map_err(error)?;
    if bootstrap != *expected {
        return Err(error("云端密钥配置已变化，请重新解锁并同步"));
    }
    for (key, _) in store
        .state
        .oss_client()
        .list_multipart_uploads("")
        .await
        .map_err(error)?
    {
        if ObjectLocations::parse(&key).is_some() {
            return Err(error("云端有未完成的附件上传，请先处理分片再改密"));
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_start_vault_key_change(
    state: State<'_, AppState>,
    plan_id: String,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    start_change(&state, plan_id, event).await
}

pub(super) async fn start_change(
    state: &AppState,
    plan_id: String,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    state.ensure_storage_available()?;
    let _guard = state
        .try_lock_key_change()
        .ok_or_else(|| error("存储正在使用，请稍后再试"))?;
    let manager = state.key_change_manager();
    let mut prepared = manager
        .prepared
        .lock()
        .await
        .take()
        .ok_or_else(|| error("请先重新统计数据和估算耗时"))?;
    if prepared.journal.transaction_id != plan_id
        || prepared.created.elapsed() > Duration::from_secs(600)
        || prepared.journal.remote != state.is_remote_enabled()
    {
        return Err(error("改密计划已过期，请重新统计"));
    }
    let store = storage(state, prepared.journal.remote);
    let mut current = store.list().await?;
    current.sort_by(|a, b| a.key.cmp(&b.key));
    if current != prepared.entries {
        return Err(error("统计后数据已变化，请重新统计后改密"));
    }
    if store.remote {
        check_cloud_start(&store, &prepared.journal.old_bootstrap).await?;
    }
    let encoded = prepared.journal.encode()?;
    // 在创建云端记录前本机先持久保存，网络响应丢失时仍然能恢复。
    state
        .persist_key_change(Some(encoded.clone()))
        .map_err(error)?;
    if store.remote {
        let created = state
            .oss_client()
            .upload_bytes_if_absent(ObjectLocations::vault_key_change(), encoded.as_bytes())
            .await
            .map_err(error)?;
        if !created {
            // 尚未替换任何对象，也未取得云锁，可以安全撤销本机的启动标记。
            state.persist_key_change(None).map_err(error)?;
            return Err(error("云端已有改密记录，停止启动以避免并发修改"));
        }
        let read_back = read_limited(
            &store,
            ObjectLocations::vault_key_change(),
            MAX_JOURNAL_BYTES,
        )
        .await?;
        if read_back != encoded.as_bytes() {
            return Err(error("云端改密记录校验失败，未开始替换数据"));
        }
    }
    execute(
        state,
        &store,
        &mut prepared.journal,
        &mut prepared.checkpoint,
        &prepared.old,
        &prepared.new,
        event,
    )
    .await
}

async fn initialize_resume_oss(
    state: &AppState,
    new: &Crypto,
    config: &LocalEncryptedConfig,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Config {
        endpoint: String,
        akid: String,
        aks: String,
        bucket: String,
    }
    let bytes = config
        .oss
        .as_ref()
        .ok_or_else(|| error("恢复记录缺少 OSS 配置"))?;
    let plain = Zeroizing::new(new.decrypt(bytes).map_err(error)?);
    let config: Config = serde_json::from_slice(&plain).map_err(error)?;
    state
        .oss_client()
        .initialize(config.endpoint, config.akid, config.aks, config.bucket)
        .map_err(error)
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_resume_vault_key_change(
    state: State<'_, AppState>,
    password: String,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    resume_change(&state, password, event).await
}

pub(super) async fn resume_change(
    state: &AppState,
    password: String,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    let _guard = state
        .try_lock_key_change()
        .ok_or_else(|| error("改密任务正在运行"))?;
    let encoded = state
        .pending_key_change()
        .ok_or_else(|| error("没有需要续改的任务"))?;
    let mut journal = Journal::parse(&encoded)?;
    let (old, new, mut checkpoint) = journal.open(password)?;
    let store = storage(state, journal.remote);
    if store.remote {
        initialize_resume_oss(state, &new, &checkpoint.local_config).await?;
        state
            .oss_client()
            .require_unversioned_bucket()
            .await
            .map_err(error)?;
        verify_cloud_bootstrap(&store, &journal, false).await?;
        if store
            .head(ObjectLocations::vault_key_change())
            .await?
            .is_some()
        {
            let remote = read_limited(
                &store,
                ObjectLocations::vault_key_change(),
                MAX_JOURNAL_BYTES,
            )
            .await?;
            let remote = Journal::parse(std::str::from_utf8(&remote).map_err(error)?)?;
            verify_same_transaction(&journal, &remote)?;
        } else {
            // 云记录创建响应丢失或最终确认中断时，本机仍保留检查点。
            // 重新取得云锁后才允许恢复，不能直接覆盖可能由另一设备创建的记录。
            let created = state
                .oss_client()
                .upload_bytes_if_absent(ObjectLocations::vault_key_change(), encoded.as_bytes())
                .await
                .map_err(error)?;
            if !created {
                return Err(error("云端已出现其他改密记录，请停止并检查其他设备"));
            }
            let saved = read_limited(
                &store,
                ObjectLocations::vault_key_change(),
                MAX_JOURNAL_BYTES,
            )
            .await?;
            if saved != encoded.as_bytes() {
                return Err(error("重新创建的云端改密记录校验失败"));
            }
        }
        cleanup_interrupted_uploads(&store, &journal, &checkpoint).await?;
    }
    execute(
        state,
        &store,
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        event,
    )
    .await
}

fn verify_same_transaction(expected: &Journal, actual: &Journal) -> Result<()> {
    if actual.transaction_id != expected.transaction_id
        || actual.remote != expected.remote
        || actual.old_bootstrap != expected.old_bootstrap
        || actual.new_bootstrap != expected.new_bootstrap
    {
        return Err(error("云端改密任务与本机不同，拒绝修改或清理"));
    }
    Ok(())
}

async fn verify_cloud_bootstrap(store: &Storage, journal: &Journal, finished: bool) -> Result<()> {
    let current = read_limited(store, ObjectLocations::vault_bootstrap(), 16 * 1024).await?;
    let current = crate::vault_bootstrap::VaultBootstrap::from_json(
        std::str::from_utf8(&current).map_err(error)?,
    )
    .map_err(error)?;
    if current != journal.new_bootstrap && (finished || current != journal.old_bootstrap) {
        return Err(error("云端 Vault 已变化，拒绝覆盖或清理密钥配置"));
    }
    Ok(())
}

pub(super) async fn cleanup_interrupted_uploads(
    store: &Storage,
    journal: &Journal,
    checkpoint: &Checkpoint,
) -> Result<()> {
    let prefix = ObjectLocations::vault_key_change_staging_prefix(&journal.transaction_id);
    for (key, upload_id) in store
        .state
        .oss_client()
        .list_multipart_uploads("")
        .await
        .map_err(error)?
    {
        let current_target = checkpoint
            .items
            .get(checkpoint.next)
            .map(|i| i.source.key.as_str());
        if key.starts_with(&prefix)
            || (checkpoint.staged.is_some() && Some(key.as_str()) == current_target)
        {
            store
                .state
                .oss_client()
                .abort_multipart_upload(&key, &upload_id)
                .await
                .map_err(error)?;
        }
    }
    Ok(())
}

async fn execute(
    state: &AppState,
    store: &Storage,
    journal: &mut Journal,
    checkpoint: &mut Checkpoint,
    old: &Crypto,
    new: &Crypto,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    let manager = state.key_change_manager();
    manager.paused.store(false, Ordering::SeqCst);
    let progress_event = event.clone();
    run(
        store,
        journal,
        checkpoint,
        old,
        new,
        move |p| {
            let _ = progress_event.send(p);
        },
        || manager.paused.load(Ordering::SeqCst),
    )
    .await?;
    checkpoint.data_committed = true;
    journal.update(new, checkpoint)?;
    store.checkpoint(&journal.encode()?).await?;
    let _ = event.send(progress(checkpoint, "switchingKey"));
    if store.remote {
        let current = read_limited(store, ObjectLocations::vault_bootstrap(), 16 * 1024).await?;
        let current = crate::vault_bootstrap::VaultBootstrap::from_json(
            std::str::from_utf8(&current).map_err(error)?,
        )
        .map_err(error)?;
        if current != journal.old_bootstrap && current != journal.new_bootstrap {
            return Err(error("云端 Vault 已变化，拒绝覆盖密钥配置"));
        }
        let bytes = serde_json::to_vec(&journal.new_bootstrap).map_err(error)?;
        state
            .oss_client()
            .upload_bytes(ObjectLocations::vault_bootstrap(), &bytes)
            .await
            .map_err(error)?;
        if read_limited(store, ObjectLocations::vault_bootstrap(), 16 * 1024).await? != bytes {
            return Err(error("云端新密钥配置读回校验失败"));
        }
    }
    if store.remote || checkpoint.refresh_cloud_cache {
        state.attachment_cache().deactivate().await.map_err(error)?;
        // 云缓存不是权威数据。删掉旧密文缓存，避免它们被当作新密钥的数据读取。
        for entry in state
            .local_object_store()
            .get_all_entries()
            .await
            .map_err(error)?
        {
            if ObjectLocations::parse(&entry.key).is_some() {
                state
                    .local_object_store()
                    .delete(&entry.key)
                    .await
                    .map_err(error)?;
            }
        }
    }
    state
        .persist_vault_bootstrap(journal.new_bootstrap.clone())
        .map_err(error)?;
    state.crypto().adopt(new).map_err(error)?;
    state.diary_cache().clear();
    state.ai_session_repository().invalidate_reconciliation();
    let _ = event.send(progress(checkpoint, "savingLocalConfig"));
    Ok(checkpoint.local_config.clone())
}

/// 在普通云同步或首次启用前检测恢复记录，避免旧密钥写入混合状态的数据。
pub(crate) async fn ensure_no_remote_key_change(state: &AppState) -> Result<()> {
    if state
        .oss_client()
        .object_exists(ObjectLocations::vault_key_change())
        .await
        .map_err(error)?
    {
        return Err(error(
            "云端正在改密，请先在发起改密的设备完成续改，再重新解锁此设备",
        ));
    }
    Ok(())
}

/// 其他设备完成改密后：先用本机旧密码解锁 OSS 配置，再用新密码确认云端 Vault。
/// 本机更新也使用恢复记录，防止新 bootstrap 已保存、前端凭据还未更新时退出。
#[tauri::command]
#[specta::specta]
pub async fn cmd_sync_changed_vault_key(
    state: State<'_, AppState>,
    new_password: String,
    local_config: LocalEncryptedConfig,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    sync_changed_key(&state, new_password, local_config, event).await
}

pub(super) async fn sync_changed_key(
    state: &AppState,
    new_password: String,
    local_config: LocalEncryptedConfig,
    event: Channel<KeyChangeProgress>,
) -> Result<LocalEncryptedConfig> {
    state.ensure_storage_available()?;
    let _guard = state
        .try_lock_key_change()
        .ok_or_else(|| error("存储正在使用"))?;
    if state.configured_remote_enabled() != Some(true) {
        return Err(error("本机没有启用云同步"));
    }
    ensure_no_remote_key_change(state).await?;
    let old_bootstrap = state
        .vault_bootstrap()
        .ok_or_else(|| error("本机缺少密钥配置"))?;
    let store = storage(state, true);
    let remote = read_limited(&store, ObjectLocations::vault_bootstrap(), 16 * 1024).await?;
    let new_bootstrap = crate::vault_bootstrap::VaultBootstrap::from_json(
        std::str::from_utf8(&remote).map_err(error)?,
    )
    .map_err(error)?;
    if old_bootstrap.vault_id != new_bootstrap.vault_id || old_bootstrap.kdf == new_bootstrap.kdf {
        return Err(error("不是同一 Vault 的改密，不能覆盖本机密钥配置"));
    }
    let old = state.crypto().snapshot().map_err(error)?;
    let new = Crypto::new();
    new.derive_and_verify_bootstrap(new_password, &new_bootstrap)
        .map_err(error)?;
    let config = rotate_local_config(&old, &new, local_config)?;
    let (mut journal, mut checkpoint) = Journal::create(
        &old,
        &new,
        old_bootstrap,
        new_bootstrap,
        false,
        vec![],
        config,
    )?;
    checkpoint.refresh_cloud_cache = true;
    journal.update(&new, &checkpoint)?;
    state
        .persist_key_change(Some(journal.encode()?))
        .map_err(error)?;
    execute(
        state,
        &storage(state, false),
        &mut journal,
        &mut checkpoint,
        &old,
        &new,
        event,
    )
    .await
}

/// 前端持久保存加密配置并禁用旧生物凭据后，才删除恢复密钥桥并解除改密锁。
#[tauri::command]
#[specta::specta]
pub async fn cmd_ack_vault_key_change(
    state: State<'_, AppState>,
    local_config: LocalEncryptedConfig,
) -> Result<()> {
    ack_change(&state, local_config).await
}

pub(super) async fn ack_change(state: &AppState, local_config: LocalEncryptedConfig) -> Result<()> {
    let _guard = state
        .try_lock_key_change()
        .ok_or_else(|| error("改密任务正在运行"))?;
    let Some(encoded) = state.pending_key_change() else {
        return Ok(());
    };
    let journal = Journal::parse(&encoded)?;
    let new = state.crypto();
    let checkpoint = journal.checkpoint(&new)?;
    if !checkpoint.data_committed
        || checkpoint.next != checkpoint.items.len()
        || checkpoint.local_config != local_config
        || state.vault_bootstrap().as_ref() != Some(&journal.new_bootstrap)
    {
        return Err(error("本机配置尚未完成保存，不能结束改密"));
    }
    let store = storage(state, journal.remote);
    if store.remote {
        verify_cloud_bootstrap(&store, &journal, true).await?;
        if store
            .head(ObjectLocations::vault_key_change())
            .await?
            .is_some()
        {
            let saved = read_limited(
                &store,
                ObjectLocations::vault_key_change(),
                MAX_JOURNAL_BYTES,
            )
            .await?;
            let remote = Journal::parse(std::str::from_utf8(&saved).map_err(error)?)?;
            verify_same_transaction(&journal, &remote)?;
        }
    }
    // 恢复时清理已经提交、但上次删除失败的临时对象。任何删除失败都保留记录供重试。
    // 只枚举本次事务剩余的对象，避免为每个已清理文件重复发出两个 DELETE。
    let prefix = ObjectLocations::vault_key_change_staging_prefix(&journal.transaction_id);
    for entry in store.list_prefix(&prefix).await? {
        let name = entry
            .key
            .strip_prefix(&prefix)
            .ok_or_else(|| error("临时对象位置无效"))?;
        let index = name
            .strip_suffix(".old.enc")
            .or_else(|| name.strip_suffix(".enc"))
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|i| *i < checkpoint.items.len())
            .ok_or_else(|| error("改密临时目录包含未知对象，不能自动清理"))?;
        if entry.key != ObjectLocations::vault_key_change_stage(&journal.transaction_id, index)
            && entry.key != ObjectLocations::vault_key_change_backup(&journal.transaction_id, index)
        {
            return Err(error("改密临时对象名称无效"));
        }
        store.delete(&entry.key).await?;
    }
    if store.remote {
        store.delete(ObjectLocations::vault_key_change()).await?;
    }
    state.persist_key_change(None).map_err(error)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cmd_get_vault_key_change_status(state: State<'_, AppState>) -> Result<KeyChangeStatus> {
    let journal = state
        .pending_key_change()
        .map(|json| Journal::parse(&json))
        .transpose()?;
    Ok(KeyChangeStatus {
        pending: journal.is_some(),
        remote: journal.is_some_and(|j| j.remote),
    })
}

#[tauri::command]
#[specta::specta]
pub fn cmd_pause_vault_key_change(state: State<'_, AppState>) {
    state.key_change_manager().request_pause();
}

#[tauri::command]
#[specta::specta]
pub async fn cmd_discard_vault_key_change_plan(state: State<'_, AppState>) -> Result<()> {
    *state.key_change_manager().prepared.lock().await = None;
    Ok(())
}

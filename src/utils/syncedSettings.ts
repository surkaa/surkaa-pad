import type {SyncedSettingsData, SyncedSettingsDocument} from '../bindings';
import {
  CONFIG_LOCAL_CHANGE_EVENT,
  configStorageKey,
  type ConfigChangeDetail,
  type ConfigKey,
  useConfigStore,
} from '../stores/config';
import api from './api';

const SYNC_STATE_STORAGE_KEY = 'config:synced_settings_state';
const SAVE_DEBOUNCE_MS = 800;
const RETRY_DELAY_MS = 15_000;

const SYNCED_CONFIG_KEYS = [
  'app-theme',
  'default_image_size_is_small',
  'encrypt_image_attachments',
  'encrypt_audio_attachments',
  'encrypt_video_attachments',
  'encrypt_file_attachments',
  'pinned_diary_ids',
  'windows_editor_shortcuts',
  'windows_diary_list_shortcuts',
  'windows_ai_assistant_shortcuts',
  'editor_toolbar_order',
] as const satisfies readonly ConfigKey[];

const SYNCED_STORAGE_KEYS = new Set<string>(
  SYNCED_CONFIG_KEYS.map(configStorageKey),
);

interface LocalSyncState {
  dirty: boolean;
  localUpdatedAt: number;
  remoteUpdatedAt: number;
}

export interface SyncedSettingsApi {
  getStorageMode(): Promise<boolean>;
  load(): Promise<SyncedSettingsDocument | null>;
  save(settings: SyncedSettingsData): Promise<SyncedSettingsDocument>;
}

interface ReconcileOptions {
  api: SyncedSettingsApi;
  configStore: ReturnType<typeof useConfigStore>;
  syncState: LocalSyncState;
}

const commandApi: SyncedSettingsApi = {
  getStorageMode: () => api.cmdGetStorageMode(),
  load: () => api.cmdLoadSyncedSettings(),
  save: settings => api.cmdSaveSyncedSettings(settings),
};

let started = false;
let applyingRemoteSettings = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let flushPromise: Promise<void> | undefined;

function defaultSyncState(): LocalSyncState {
  return {dirty: false, localUpdatedAt: 0, remoteUpdatedAt: 0};
}

function readSyncState(): LocalSyncState {
  const raw = localStorage.getItem(SYNC_STATE_STORAGE_KEY);
  if (!raw) return defaultSyncState();
  try {
    const parsed = JSON.parse(raw) as Partial<LocalSyncState>;
    return {
      dirty: parsed.dirty === true,
      localUpdatedAt: Number.isFinite(parsed.localUpdatedAt)
        ? Math.max(0, Number(parsed.localUpdatedAt))
        : 0,
      remoteUpdatedAt: Number.isFinite(parsed.remoteUpdatedAt)
        ? Math.max(0, Number(parsed.remoteUpdatedAt))
        : 0,
    };
  } catch {
    return defaultSyncState();
  }
}

function writeSyncState(state: LocalSyncState): void {
  localStorage.setItem(SYNC_STATE_STORAGE_KEY, JSON.stringify(state));
}

export function isSyncedConfigStorageKey(key: string): boolean {
  return SYNCED_STORAGE_KEYS.has(key);
}

export async function collectSyncedSettings(
  configStore: ReturnType<typeof useConfigStore>,
): Promise<SyncedSettingsData> {
  const [
    theme,
    defaultImageSizeIsSmall,
    encryptImageAttachments,
    encryptAudioAttachments,
    encryptVideoAttachments,
    encryptFileAttachments,
    pinnedDiaryIds,
    editorShortcuts,
    diaryListShortcuts,
    aiAssistantShortcuts,
    toolbarOrder,
  ] = await Promise.all([
    configStore.getNormalConfig('app-theme'),
    configStore.getNormalConfig('default_image_size_is_small'),
    configStore.getNormalConfig('encrypt_image_attachments'),
    configStore.getNormalConfig('encrypt_audio_attachments'),
    configStore.getNormalConfig('encrypt_video_attachments'),
    configStore.getNormalConfig('encrypt_file_attachments'),
    configStore.getNormalConfig('pinned_diary_ids'),
    configStore.getNormalConfig('windows_editor_shortcuts'),
    configStore.getNormalConfig('windows_diary_list_shortcuts'),
    configStore.getNormalConfig('windows_ai_assistant_shortcuts'),
    configStore.getNormalConfig('editor_toolbar_order'),
  ]);

  return {
    appearance: {theme},
    attachments: {
      defaultImageSizeIsSmall,
      encryptImageAttachments,
      encryptAudioAttachments,
      encryptVideoAttachments,
      encryptFileAttachments,
    },
    editor: {toolbarOrder},
    pinnedDiaryIds,
    windows: {
      editorShortcuts,
      diaryListShortcuts,
      aiAssistantShortcuts,
    },
  };
}

export async function applySyncedSettings(
  configStore: ReturnType<typeof useConfigStore>,
  settings: SyncedSettingsData,
): Promise<void> {
  applyingRemoteSettings = true;
  try {
    await Promise.all([
      configStore.saveNormalConfig('app-theme', settings.appearance.theme),
      configStore.saveNormalConfig(
        'default_image_size_is_small',
        settings.attachments.defaultImageSizeIsSmall,
      ),
      configStore.saveNormalConfig(
        'encrypt_image_attachments',
        settings.attachments.encryptImageAttachments,
      ),
      configStore.saveNormalConfig(
        'encrypt_audio_attachments',
        settings.attachments.encryptAudioAttachments,
      ),
      configStore.saveNormalConfig(
        'encrypt_video_attachments',
        settings.attachments.encryptVideoAttachments,
      ),
      configStore.saveNormalConfig(
        'encrypt_file_attachments',
        settings.attachments.encryptFileAttachments,
      ),
      configStore.saveNormalConfig('pinned_diary_ids', settings.pinnedDiaryIds),
      configStore.saveNormalConfig(
        'windows_editor_shortcuts',
        settings.windows.editorShortcuts,
      ),
      configStore.saveNormalConfig(
        'windows_diary_list_shortcuts',
        settings.windows.diaryListShortcuts,
      ),
      configStore.saveNormalConfig(
        'windows_ai_assistant_shortcuts',
        settings.windows.aiAssistantShortcuts,
      ),
      configStore.saveNormalConfig('editor_toolbar_order', settings.editor.toolbarOrder),
    ]);
  } finally {
    applyingRemoteSettings = false;
  }
}

/**
 * 按更新时间执行最后写入者优先：云端不存在时上传本机设置；两端都变更时采用较新的版本。
 */
export async function reconcileSyncedSettings({
  api: settingsApi,
  configStore,
  syncState,
}: ReconcileOptions): Promise<LocalSyncState> {
  if (!(await settingsApi.getStorageMode())) return syncState;

  const remote = await settingsApi.load();
  const localWins = syncState.dirty
    && (!remote || syncState.localUpdatedAt > remote.updatedAt);
  if (!remote || localWins) {
    const saved = await settingsApi.save(await collectSyncedSettings(configStore));
    return {
      dirty: false,
      localUpdatedAt: Math.max(syncState.localUpdatedAt, saved.updatedAt),
      remoteUpdatedAt: saved.updatedAt,
    };
  }

  await applySyncedSettings(configStore, remote);
  return {
    dirty: false,
    localUpdatedAt: remote.updatedAt,
    remoteUpdatedAt: remote.updatedAt,
  };
}

function scheduleSave(delay = SAVE_DEBOUNCE_MS): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = undefined;
    void flushSettings();
  }, delay);
}

async function flushSettings(): Promise<void> {
  if (flushPromise) return flushPromise;
  flushPromise = (async () => {
    const before = readSyncState();
    if (!before.dirty) return;
    try {
      const after = await reconcileSyncedSettings({
        api: commandApi,
        configStore: useConfigStore(),
        syncState: before,
      });
      const latest = readSyncState();
      if (latest.localUpdatedAt > before.localUpdatedAt) {
        writeSyncState({...after, dirty: true, localUpdatedAt: latest.localUpdatedAt});
        scheduleSave();
      } else {
        writeSyncState(after);
      }
    } catch (error) {
      console.warn('[settings sync] save failed:', error);
      scheduleSave(RETRY_DELAY_MS);
    }
  })().finally(() => {
    flushPromise = undefined;
  });
  return flushPromise;
}

function handleConfigChange(event: Event): void {
  if (applyingRemoteSettings) return;
  const detail = (event as CustomEvent<ConfigChangeDetail>).detail;
  if (!detail || !isSyncedConfigStorageKey(detail.key)) return;
  const state = readSyncState();
  writeSyncState({...state, dirty: true, localUpdatedAt: Date.now()});
  scheduleSave();
}

export function startSyncedSettingsSync(): void {
  if (started) return;
  started = true;
  window.addEventListener(CONFIG_LOCAL_CHANGE_EVENT, handleConfigChange);
}

export async function initializeSyncedSettingsSync(): Promise<void> {
  if (saveTimer) {
    clearTimeout(saveTimer);
    saveTimer = undefined;
  }
  if (flushPromise) await flushPromise;
  const next = await reconcileSyncedSettings({
    api: commandApi,
    configStore: useConfigStore(),
    syncState: readSyncState(),
  });
  writeSyncState(next);
}

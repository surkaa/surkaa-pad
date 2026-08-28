// @vitest-environment happy-dom
import {beforeEach, describe, expect, it, vi} from 'vitest';
import {createPinia, setActivePinia} from 'pinia';
import type {
  SyncedSettingsData,
  SyncedSettingsDocument,
} from '../../bindings';
import {configStorageKey, useConfigStore} from '../../stores/config';
import {
  applySyncedSettings,
  collectSyncedSettings,
  isSyncedConfigStorageKey,
  reconcileSyncedSettings,
  type SyncedSettingsApi,
} from '../syncedSettings';

function settingsData(theme: 'light' | 'dark' | 'system' = 'dark'): SyncedSettingsData {
  return {
    appearance: {theme},
    attachments: {
      defaultImageSizeIsSmall: true,
      encryptImageAttachments: false,
      encryptAudioAttachments: true,
      encryptVideoAttachments: false,
      encryptFileAttachments: true,
    },
    editor: {
      toolbarOrder: [
        'summary',
        'taskList',
        'heading3',
        'heading2',
        'heading1',
        'strike',
        'underline',
        'bold',
      ],
    },
    pinnedDiaryIds: ['8215021834823'],
    windows: {
      editorShortcuts: {
        bold: 'Ctrl+KeyB',
        underline: 'Ctrl+KeyU',
        strike: 'Ctrl+Shift+KeyS',
        heading1: 'Ctrl+Digit1',
        heading2: 'Ctrl+Digit2',
        heading3: 'Ctrl+Digit3',
        taskList: 'Ctrl+KeyT',
        summary: 'Ctrl+Alt+KeyS',
        insertPhoto: 'Ctrl+Alt+KeyP',
        insertAudio: 'Ctrl+Alt+KeyA',
        audioRecording: 'Ctrl+Alt+KeyR',
        insertVideo: 'Ctrl+Alt+KeyV',
        insertFile: 'Ctrl+Alt+KeyF',
      },
      diaryListShortcuts: {
        createDiary: 'Ctrl+KeyN',
        aiAssistant: 'Ctrl+Alt+KeyA',
        search: 'Ctrl+KeyF',
        settings: 'Ctrl+Comma',
      },
      aiAssistantShortcuts: {focusInput: 'Ctrl+Alt+KeyI'},
    },
  };
}

function document(updatedAt: number, theme: 'light' | 'dark' | 'system' = 'dark'):
SyncedSettingsDocument {
  return {version: 1, updatedAt, ...settingsData(theme)};
}

function fakeApi(remote: SyncedSettingsDocument | null) {
  const save = vi.fn(async (settings: SyncedSettingsData) => ({
    version: 1,
    updatedAt: 300,
    ...settings,
  }));
  const api: SyncedSettingsApi = {
    getStorageMode: vi.fn(async () => true),
    load: vi.fn(async () => remote),
    save,
  };
  return {api, save};
}

describe('synced settings', () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it('collects only portable settings and excludes credentials and device settings', async () => {
    const store = useConfigStore();
    await store.saveNormalConfig('app-theme', 'light');
    await store.saveNormalConfig('biometric_enabled', true);
    await store.saveNormalConfig('encrypted_oss_config', [1, 2, 3]);
    await store.saveNormalConfig('encrypted_ai_config', [4, 5, 6]);
    await store.saveNormalConfig('attachment_upload_concurrency', 20);
    await store.saveNormalConfig('pinned_diary_ids', ['8215021834823']);

    const settings = await collectSyncedSettings(store);

    expect(settings.appearance.theme).toBe('light');
    expect(settings.pinnedDiaryIds).toEqual(['8215021834823']);
    expect(settings).not.toHaveProperty('biometricEnabled');
    expect(settings).not.toHaveProperty('encryptedOssConfig');
    expect(settings.attachments).not.toHaveProperty('uploadConcurrency');
  });

  it('applies portable settings without changing local secrets or concurrency', async () => {
    const store = useConfigStore();
    await store.saveNormalConfig('biometric_enabled', true);
    await store.saveNormalConfig('encrypted_oss_config', [1, 2, 3]);
    await store.saveNormalConfig('attachment_upload_concurrency', 17);

    await applySyncedSettings(store, settingsData());

    await expect(store.getNormalConfig('app-theme')).resolves.toBe('dark');
    await expect(store.getNormalConfig('pinned_diary_ids')).resolves.toEqual(['8215021834823']);
    await expect(store.getNormalConfig('biometric_enabled')).resolves.toBe(true);
    await expect(store.getNormalConfig('encrypted_oss_config')).resolves.toEqual([1, 2, 3]);
    await expect(store.getNormalConfig('attachment_upload_concurrency')).resolves.toBe(17);
  });

  it('uploads local settings when the cloud object does not exist', async () => {
    const {api, save} = fakeApi(null);
    const next = await reconcileSyncedSettings({
      api,
      configStore: useConfigStore(),
      syncState: {dirty: false, localUpdatedAt: 0, remoteUpdatedAt: 0},
    });

    expect(save).toHaveBeenCalledOnce();
    expect(next).toEqual({dirty: false, localUpdatedAt: 300, remoteUpdatedAt: 300});
  });

  it('keeps a newer dirty local value and uploads it', async () => {
    const store = useConfigStore();
    await store.saveNormalConfig('app-theme', 'light');
    const {api, save} = fakeApi(document(100, 'dark'));

    await reconcileSyncedSettings({
      api,
      configStore: store,
      syncState: {dirty: true, localUpdatedAt: 200, remoteUpdatedAt: 50},
    });

    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      appearance: {theme: 'light'},
    }));
  });

  it('applies a newer remote value and does not upload', async () => {
    const store = useConfigStore();
    await store.saveNormalConfig('app-theme', 'light');
    const {api, save} = fakeApi(document(200, 'dark'));

    const next = await reconcileSyncedSettings({
      api,
      configStore: store,
      syncState: {dirty: true, localUpdatedAt: 100, remoteUpdatedAt: 50},
    });

    expect(save).not.toHaveBeenCalled();
    await expect(store.getNormalConfig('app-theme')).resolves.toBe('dark');
    expect(next).toEqual({dirty: false, localUpdatedAt: 200, remoteUpdatedAt: 200});
  });

  it('recognizes only the explicitly portable config keys', () => {
    expect(isSyncedConfigStorageKey(configStorageKey('app-theme'))).toBe(true);
    expect(isSyncedConfigStorageKey(configStorageKey('editor_toolbar_order'))).toBe(true);
    expect(isSyncedConfigStorageKey(configStorageKey('pinned_diary_ids'))).toBe(true);
    expect(isSyncedConfigStorageKey(configStorageKey('biometric_dek'))).toBe(false);
    expect(isSyncedConfigStorageKey(configStorageKey('encrypted_oss_config'))).toBe(false);
    expect(isSyncedConfigStorageKey(configStorageKey('attachment_upload_concurrency'))).toBe(false);
  });
});

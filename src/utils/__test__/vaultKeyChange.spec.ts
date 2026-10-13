// @vitest-environment happy-dom
import {beforeEach, describe, expect, it, vi} from 'vitest';
import {createPinia, setActivePinia} from 'pinia';
import {useConfigStore} from '../../stores/config';
import {estimateDuration, finishVaultKeyChange, getLocalEncryptedConfig, keyChangeCompletionMessage, keyChangeFileProgressLabel, keyChangeFileProgressValue, keyChangeProgressLabel, keyChangeProgressValue} from '../vaultKeyChange';
import type {LocalEncryptedConfig} from '../../bindings';

const api = vi.hoisted(() => ({cmdAckVaultKeyChange: vi.fn(async (_config: LocalEncryptedConfig) => {})}));
vi.mock('../api', () => ({default: api}));

const config: LocalEncryptedConfig = {vaultVerifier: [1, 2], oss: [3, 4], ai: [5, 6]};
describe('vault key change local commit', () => {
  beforeEach(() => { localStorage.clear(); setActivePinia(createPinia()); api.cmdAckVaultKeyChange.mockReset().mockResolvedValue(undefined); });

  it('saves all encrypted configuration and disables old biometric credentials before acknowledgement', async () => {
    const store = useConfigStore();
    await store.saveNormalConfig('biometric_enabled', true);
    await store.saveNormalConfig('biometric_dek', 'old-biometric-key');
    await store.saveNormalConfig('last_password_unlock_at', Date.now());
    api.cmdAckVaultKeyChange.mockImplementationOnce(async saved => {
      expect(saved).toEqual(config);
      expect(await store.getNormalConfig('biometric_enabled')).toBe(false);
      expect(await store.getNormalConfig('biometric_dek')).toBeNull();
      expect(await getLocalEncryptedConfig()).toEqual(config);
    });
    expect(await finishVaultKeyChange(config)).toBe(true);
    expect(api.cmdAckVaultKeyChange).toHaveBeenCalledExactlyOnceWith(config);
  });

  it('does not acknowledge partial local persistence and safely retries it', async () => {
    const store = useConfigStore();
    const save = store.saveNormalConfig.bind(store);
    const spy = vi.spyOn(store, 'saveNormalConfig').mockImplementation(async (key, value) => {
      if (key === 'encrypted_ai_config') throw new Error('磁盘空间不足');
      await save(key, value);
    });
    await expect(finishVaultKeyChange(config)).rejects.toThrow('磁盘空间不足');
    expect(api.cmdAckVaultKeyChange).not.toHaveBeenCalled();
    spy.mockRestore();
    await finishVaultKeyChange(config);
    expect(await getLocalEncryptedConfig()).toEqual(config);
  });

  it('is idempotent after an acknowledgement response is lost', async () => {
    api.cmdAckVaultKeyChange.mockRejectedValueOnce(new Error('响应丢失'));
    await expect(finishVaultKeyChange(config)).rejects.toThrow();
    await finishVaultKeyChange(config);
    expect(await getLocalEncryptedConfig()).toEqual(config);
    expect(api.cmdAckVaultKeyChange).toHaveBeenCalledTimes(2);
  });

  it('handles null optional encrypted configuration', async () => {
    const empty: LocalEncryptedConfig = {vaultVerifier: null, oss: null, ai: null};
    await finishVaultKeyChange(empty);
    expect(await getLocalEncryptedConfig()).toEqual(empty);
  });

  it('only mentions disabled biometric unlock when it was actually enabled', async () => {
    expect(await finishVaultKeyChange(config)).toBe(false);
    expect(keyChangeCompletionMessage(false)).toBe('改密已完成');
    await useConfigStore().saveNormalConfig('biometric_enabled', true);
    expect(await finishVaultKeyChange(config)).toBe(true);
    expect(keyChangeCompletionMessage(true)).toContain('生物解锁已关闭');
  });
});

describe('key change progress and estimates', () => {
  it('uses a bounded zero-to-one value and does not confuse data completion with key switching', () => {
    const p = {phase: 'switchingKey', completedFiles: 2, totalFiles: 2, completedBytes: 200, totalBytes: 200, currentFile: null, fileProgress: null};
    expect(keyChangeProgressValue(p)).toBe(1);
    expect(keyChangeProgressLabel(p.phase)).toContain('切换密钥');
    expect(keyChangeProgressValue({...p, completedBytes: 999})).toBe(1);
    expect(keyChangeProgressValue({...p, totalBytes: 0, totalFiles: 0})).toBe(0);
    expect(keyChangeProgressValue({...p, completedBytes: -1})).toBe(0);
  });
  it('shows the quantized points rather than pretending consumed bytes are verified completion', () => {
    const p = {step: 'checkingStageContent' as const, path: 'full' as const, completedBytes: 100, totalBytes: 100, completedPoints: 9, totalPoints: 10};
    expect(keyChangeFileProgressValue(p)).toBe(0.9);
    expect(keyChangeFileProgressValue({...p, completedPoints: 10})).toBe(1);
    expect(keyChangeFileProgressValue({...p, completedPoints: 31, totalPoints: 100})).toBe(0.31);
    expect(keyChangeFileProgressValue({...p, completedPoints: -1})).toBe(0);
    expect(keyChangeFileProgressValue({...p, totalPoints: 0})).toBe(0);
    expect(keyChangeFileProgressValue({...p, completedPoints: NaN})).toBe(0);
    expect(keyChangeFileProgressLabel(p.step)).toContain('校验新内容');
    expect(keyChangeFileProgressLabel('checkingCommitted')).toContain('校验已替换');
  });
  it('formats measured time without implying a precise wall-clock ETA', () => {
    expect(estimateDuration(200)).toBe('约 1 秒');
    expect(estimateDuration(61000)).toBe('约 2 分钟');
    expect(estimateDuration(7200000)).toBe('约 2.0 小时');
    expect(estimateDuration(NaN)).toBe('无法估算');
  });
});

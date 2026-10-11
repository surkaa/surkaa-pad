import api from './api';
import {useConfigStore} from '../stores/config';
import type {LocalEncryptedConfig, KeyChangeProgress} from '../bindings';

export async function getLocalEncryptedConfig(): Promise<LocalEncryptedConfig> {
  const store = useConfigStore();
  return {
    vaultVerifier: await store.getNormalConfig('vault_verifier'),
    oss: await store.getNormalConfig('encrypted_oss_config'),
    ai: await store.getNormalConfig('encrypted_ai_config'),
  };
}

/** 后端保留恢复记录，直到这些独立的本机写入全部完成。重复执行也是安全的。 */
export async function finishVaultKeyChange(config: LocalEncryptedConfig) {
  const store = useConfigStore();
  // 先禁用旧生物凭据，防止保存配置期间重启时用旧 DEK 自动登录。
  await store.saveNormalConfig('biometric_enabled', false);
  await store.deleteConfig('biometric_dek', 'last_password_unlock_at');
  await store.saveNormalConfig('vault_verifier', config.vaultVerifier);
  await store.saveNormalConfig('encrypted_oss_config', config.oss);
  await store.saveNormalConfig('encrypted_ai_config', config.ai);
  const saved = await getLocalEncryptedConfig();
  if (JSON.stringify(saved) !== JSON.stringify(config)) {
    throw new Error('本机加密配置尚未保存完成，请继续改密重试');
  }
  await api.cmdAckVaultKeyChange(saved);
}

export function keyChangeProgressLabel(phase: string): string {
  const labels: Record<string, string> = {
    encrypting: '正在重加密', verifying: '正在读回校验', committing: '正在替换已校验的数据',
    completedObject: '已保存检查点', switchingKey: '数据已处理，正在切换密钥配置',
    savingLocalConfig: '正在保存本机配置',
  };
  return labels[phase] ?? '正在改密';
}

export function keyChangeProgressValue(progress: KeyChangeProgress): number {
  const value = progress.totalBytes > 0 ? progress.completedBytes / progress.totalBytes
    : progress.totalFiles > 0 ? progress.completedFiles / progress.totalFiles : 0;
  return Math.max(0, Math.min(value, 1));
}

export function estimateDuration(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return '无法估算';
  const seconds = Math.max(1, Math.ceil(milliseconds / 1000));
  if (seconds < 60) return `约 ${seconds} 秒`;
  if (seconds < 3600) return `约 ${Math.ceil(seconds / 60)} 分钟`;
  return `约 ${(seconds / 3600).toFixed(1)} 小时`;
}

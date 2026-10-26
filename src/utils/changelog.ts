import {getIdentifier, getName} from '@tauri-apps/api/app';
import {platform} from '@tauri-apps/plugin-os';
import {isDevelopmentEdition} from './vaultKdfSetup';

export type ChangelogGroup = 'stable' | 'windowsDev' | 'androidDev';
export const CHANGELOG_GROUP_LABELS: Record<ChangelogGroup, string> = {
  stable: '正式版', windowsDev: 'Windows Dev', androidDev: 'Android Dev',
};

export interface ChangelogEnvironment {
  frontendDevelopment: boolean;
  getIdentifier(): Promise<string>;
  getName(): Promise<string>;
  getPlatform(): string;
}

/** 以应用版本和实际平台选择唯一日志；Dev 的 Release 安装包不能仅依赖 Vite 的 DEV 标记。 */
export async function loadChangelogGroup(source: ChangelogEnvironment = {
  frontendDevelopment: import.meta.env.DEV, getIdentifier, getName, getPlatform: platform,
}): Promise<ChangelogGroup> {
  let development = source.frontendDevelopment;
  if (!development) {
    const [identifier, name] = await Promise.all([source.getIdentifier(), source.getName()]);
    development = isDevelopmentEdition(identifier, name, false);
  }
  if (!development) return 'stable';
  const currentPlatform = source.getPlatform();
  if (currentPlatform === 'windows') return 'windowsDev';
  if (currentPlatform === 'android') return 'androidDev';
  // 不把其他平台的开发记录误展示为当前平台日志。
  throw new Error(`暂无当前平台的开发版更新日志：${currentPlatform}`);
}

/** 使用设备本地时区展示完整日期，避免旧版本记录变成不明确的相对时间。 */
export function formatChangelogTimestamp(timestamp: number): string {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return '时间未知';
  return date.toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', hour12: false,
  });
}

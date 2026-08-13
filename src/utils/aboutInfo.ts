import {getIdentifier, getName, getTauriVersion, getVersion} from '@tauri-apps/api/app';
import {arch, platform, version as getOsVersion} from '@tauri-apps/plugin-os';

export interface AboutInfo {
  appName: string;
  appVersion: string;
  gitCommit: string;
  identifier: string;
  tauriVersion: string;
  platform: string;
  architecture: string;
  osVersion: string;
}

export interface AboutInfoSource {
  getName(): Promise<string>;
  getVersion(): Promise<string>;
  getIdentifier(): Promise<string>;
  getTauriVersion(): Promise<string>;
  getPlatform(): string;
  getArchitecture(): string;
  getOsVersion(): string;
  gitCommit: string;
}

const PLATFORM_LABELS: Record<string, string> = {
  android: 'Android',
  ios: 'iOS',
  linux: 'Linux',
  macos: 'macOS',
  windows: 'Windows',
};

export async function loadAboutInfo(source: AboutInfoSource = defaultSource()): Promise<AboutInfo> {
  const [appName, appVersion, identifier, tauriVersion] = await Promise.all([
    source.getName(),
    source.getVersion(),
    source.getIdentifier(),
    source.getTauriVersion(),
  ]);
  const rawPlatform = source.getPlatform();
  return {
    appName,
    appVersion,
    gitCommit: normalizeGitCommit(source.gitCommit),
    identifier,
    tauriVersion,
    platform: PLATFORM_LABELS[rawPlatform] || rawPlatform,
    architecture: source.getArchitecture(),
    osVersion: source.getOsVersion(),
  };
}

export function formatAboutSummary(info: AboutInfo): string {
  return `版本 ${info.appVersion} · ${info.gitCommit}`;
}

export function normalizeGitCommit(commit: string): string {
  const normalized = commit.trim();
  return normalized && normalized !== 'unknown' ? normalized.slice(0, 8) : '未知提交';
}

function defaultSource(): AboutInfoSource {
  return {
    getName,
    getVersion,
    getIdentifier,
    getTauriVersion,
    getPlatform: platform,
    getArchitecture: arch,
    getOsVersion,
    gitCommit: typeof __APP_GIT_COMMIT__ === 'string' ? __APP_GIT_COMMIT__ : 'unknown',
  };
}

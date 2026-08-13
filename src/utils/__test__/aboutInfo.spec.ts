import {describe, expect, it} from 'vitest';
import {
  formatAboutSummary,
  loadAboutInfo,
  normalizeGitCommit,
  type AboutInfoSource,
} from '../aboutInfo';

function source(overrides: Partial<AboutInfoSource> = {}): AboutInfoSource {
  return {
    getName: async () => 'SurKaa Pad',
    getVersion: async () => '0.8.0',
    getIdentifier: async () => 'cn.surkaa.pad',
    getTauriVersion: async () => '2.9.5',
    getPlatform: () => 'windows',
    getArchitecture: () => 'x86_64',
    getOsVersion: () => '10.0.26100',
    gitCommit: '1234567890abcdef',
    ...overrides,
  };
}

describe('about info', () => {
  it('loads runtime and build information from one source', async () => {
    await expect(loadAboutInfo(source())).resolves.toEqual({
      appName: 'SurKaa Pad',
      appVersion: '0.8.0',
      gitCommit: '12345678',
      identifier: 'cn.surkaa.pad',
      tauriVersion: '2.9.5',
      platform: 'Windows',
      architecture: 'x86_64',
      osVersion: '10.0.26100',
    });
  });

  it('keeps unknown platforms readable and normalizes missing commits', async () => {
    const info = await loadAboutInfo(source({
      getPlatform: () => 'freebsd',
      gitCommit: 'unknown',
    }));

    expect(info.platform).toBe('freebsd');
    expect(info.gitCommit).toBe('未知提交');
    expect(normalizeGitCommit('  ')).toBe('未知提交');
  });

  it('formats a concise settings summary', async () => {
    const info = await loadAboutInfo(source());
    expect(formatAboutSummary(info)).toBe('版本 0.8.0 · 12345678');
  });
});

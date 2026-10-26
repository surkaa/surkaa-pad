import {afterEach, describe, expect, it, vi} from 'vitest';
import {formatChangelogTimestamp, loadChangelogGroup, type ChangelogEnvironment} from '../changelog';

afterEach(() => vi.restoreAllMocks());

describe('更新日志环境选择', () => {
  function source(overrides: Partial<ChangelogEnvironment> = {}): ChangelogEnvironment {
    return {
      frontendDevelopment: false,
      getIdentifier: vi.fn(async () => 'cn.surkaa.pad'),
      getName: vi.fn(async () => 'SurKaa Pad'),
      getPlatform: vi.fn(() => 'windows'),
      ...overrides,
    };
  }

  it.each(['windows', 'android', 'linux'])('正式安装包在 %s 只显示正式记录', async currentPlatform => {
    const environment = source({getPlatform: vi.fn(() => currentPlatform)});
    expect(await loadChangelogGroup(environment)).toBe('stable');
    expect(environment.getPlatform).not.toHaveBeenCalled();
  });

  it.each([['windows', 'windowsDev'], ['android', 'androidDev']])('开发服务器 %s 只选择所在平台', async (currentPlatform, expected) => {
    const environment = source({frontendDevelopment: true, getPlatform: () => currentPlatform});
    expect(await loadChangelogGroup(environment)).toBe(expected);
    expect(environment.getIdentifier).not.toHaveBeenCalled();
    expect(environment.getName).not.toHaveBeenCalled();
  });

  it('Windows Dev 的 Release 安装包通过独立应用标识识别', async () => {
    expect(await loadChangelogGroup(source({getIdentifier: async () => 'cn.surkaa.pad.dev'}))).toBe('windowsDev');
  });

  it('Android Dev 的 Release 安装包通过产品名识别，不误用共享的正式应用标识', async () => {
    expect(await loadChangelogGroup(source({getName: async () => 'SurKaa Pad (Dev)', getPlatform: () => 'android'})))
      .toBe('androidDev');
  });

  it('不因类似的产品名或标识错误识别为开发版', async () => {
    expect(await loadChangelogGroup(source({
      getIdentifier: async () => 'cn.surkaa.pad.development', getName: async () => 'SurKaa Pad (Dev) backup',
    }))).toBe('stable');
  });

  it.each(['getIdentifier', 'getName'] as const)('读取 %s 失败时不猜测版本类型', async method => {
    const environment = source({[method]: async () => { throw new Error('读取失败'); }});
    await expect(loadChangelogGroup(environment)).rejects.toThrow('读取失败');
  });

  it('不支持的平台不显示 Windows 或 Android 开发记录', async () => {
    await expect(loadChangelogGroup(source({frontendDevelopment: true, getPlatform: () => 'linux'})))
      .rejects.toThrow('暂无当前平台的开发版更新日志');
  });
});

describe('更新日志时间', () => {
  it('使用设备本地时区和完整日期，不强制 UTC 或相对时间', () => {
    const timestamp = 1793003388000;
    const format = vi.spyOn(Date.prototype, 'toLocaleString');
    const result = formatChangelogTimestamp(timestamp);
    expect(format).toHaveBeenCalledWith('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false,
    });
    expect(format.mock.calls[0][1]).not.toHaveProperty('timeZone');
    expect(result).toContain('2026');
    expect(result).not.toMatch(/UTC|GMT|刚刚|分钟前/);
  });

  it.each([0, 1763789654000, 1793003388000])('格式化有效毫秒时间戳 %s', timestamp => {
    expect(formatChangelogTimestamp(timestamp)).toBe(new Date(timestamp).toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false,
    }));
  });

  it.each([NaN, Infinity, -Infinity, 8_640_000_000_000_001])('无效时间 %s 有安全兜底', timestamp => {
    expect(formatChangelogTimestamp(timestamp)).toBe('时间未知');
  });
});

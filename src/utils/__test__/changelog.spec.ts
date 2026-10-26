import {afterEach, describe, expect, it, vi} from 'vitest';
import {formatChangelogTimestamp} from '../changelog';

afterEach(() => vi.restoreAllMocks());

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

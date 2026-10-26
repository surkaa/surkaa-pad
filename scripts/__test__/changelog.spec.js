import {readFileSync, realpathSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {describe, expect, it} from 'vitest';

const root = realpathSync(fileURLToPath(new URL('../../', import.meta.url)));
const readJson = path => JSON.parse(readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8'));
const changelog = readJson('src/data/changelog.json');
const configGroups = {
  'src-tauri/tauri.conf.json': 'stable',
  'src-tauri/tauri.dev.json': 'windowsDev',
  'src-tauri/tauri.android-dev.json': 'androidDev',
};
const entries = Object.entries(changelog).flatMap(([group, records]) => records.map(record => ({group, record})));
const git = (...args) => execFileSync('git', args, {cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe']});
let fullHistory = false;
try { fullHistory = git('rev-parse', '--is-shallow-repository').trim() === 'false'; }
catch { /* 源码压缩包中仍运行格式检查，不依赖 Git。 */ }

describe('应用更新记录', () => {
  it('只按正式、Windows Dev 和 Android Dev 分组，且各组非空', () => {
    expect(Object.keys(changelog).sort()).toEqual(['androidDev', 'stable', 'windowsDev']);
    for (const records of Object.values(changelog)) {
      expect(Array.isArray(records)).toBe(true);
      expect(records.length).toBeGreaterThan(0);
    }
  });

  it.each(entries)('$group $record.version 仅包含时间戳、版本号与不超过100字的描述', ({record}) => {
    expect(Object.keys(record).sort()).toEqual(['description', 'updatedAt', 'version']);
    expect(Number.isSafeInteger(record.updatedAt)).toBe(true);
    expect(record.updatedAt).toBeGreaterThan(Date.UTC(2020, 0, 1));
    expect(Number.isNaN(new Date(record.updatedAt).getTime())).toBe(false);
    expect(record.version).toMatch(/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/);
    expect(typeof record.description).toBe('string');
    expect(record.description.trim()).toBe(record.description);
    expect([...record.description].length).toBeGreaterThan(0);
    expect([...record.description].length).toBeLessThanOrEqual(100);
    expect(record.description).not.toMatch(/[\r\n<>]/);
  });

  it('每个版本序列去重，并按版本号倒序；不再保留预发布版本', () => {
    for (const records of Object.values(changelog)) {
      const versions = records.map(record => record.version);
      expect(new Set(versions).size).toBe(versions.length);
      expect(versions).toEqual([...versions].sort((a, b) => b.localeCompare(a, 'en', {numeric: true})));
      for (const version of versions) expect(version.includes('-')).toBe(false);
    }
  });

  it('覆盖三个当前应用配置版本，且不误用工程包版本', () => {
    for (const [path, group] of Object.entries(configGroups)) {
      expect(changelog[group][0].version).toBe(readJson(path).version);
    }
    expect(changelog.windowsDev.find(record => record.version === '0.4.10').description)
      .not.toBe(changelog.stable.find(record => record.version === '0.4.10').description);
  });

  it.skipIf(!fullHistory)('保留本次整理的全部 Git 历史版本及首次版本变更时间', () => {
    // 冻结历史整理基线；未来新增版本可以与 JSON 同次提交，不要求预知提交时间。
    const log = git('log', '8918b61', '--reverse', '--topo-order', '--format=%x1e%H|%at',
      '--no-renames', '-G', '"version"', '-p', '--', ...Object.keys(configGroups));
    const history = new Map();
    for (const chunk of log.split('\x1e').slice(1)) {
      const [metadata, ...lines] = chunk.split('\n');
      const [, seconds] = metadata.split('|');
      let path = '';
      for (const line of lines) {
        if (line.startsWith('diff --git ')) path = line.split(' b/')[1];
        const match = line.match(/^\+\s*"version":\s*"([^"]+)"/);
        if (!match) continue;
        // 已按产品要求移除预览版，只审计保留的正式/平台开发版历史。
        if (match[1].includes('-')) continue;
        const group = configGroups[path];
        const key = `${group}:${match[1]}`;
        if (!history.has(key)) history.set(key, Number(seconds) * 1000);
      }
    }
    // 最早标签与配置不一致，单独验证这个唯一的已知历史例外。
    const legacyRef = '6333073'; // v0.0.1 指向此提交；无本地标签引用也能检查完整克隆。
    history.set('stable:0.0.1', Number(git('show', '-s', '--format=%at', legacyRef).trim()) * 1000);
    expect(JSON.parse(git('show', `${legacyRef}:src-tauri/tauri.conf.json`)).version).toBe('0.1.0');
    expect(changelog.stable.find(record => record.version === '0.0.1').description).toContain('实际为0.1.0');
    const actual = new Map(entries.map(({group, record}) => [`${group}:${record.version}`, record]));
    for (const [key, timestamp] of history) {
      expect(actual.has(key), `缺少历史版本 ${key}`).toBe(true);
      expect(actual.get(key).updatedAt, key).toBe(timestamp);
    }
  });
});

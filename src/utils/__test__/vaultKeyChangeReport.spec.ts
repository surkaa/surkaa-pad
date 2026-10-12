import {describe, expect, it} from 'vitest';
import {keyChangeReportSource, keyChangeReportOutcome, reportDuration} from '../vaultKeyChangeReport';
import {reportFixture} from './fixtures/vaultKeyChangeReport';

describe('vault key change performance report', () => {
  it('preserves all raw measurements and adds clear scope notes without copying unrelated context', () => {
    const report = reportFixture();
    const context = {appVersion: '0.8.11', gitCommit: '12345678', apiKey: 'must-not-be-copied'};
    const source = keyChangeReportSource(report, context);
    expect(source.measurements).toBe(report);
    expect(source.application).toEqual({version: '0.8.11', gitCommit: '12345678'});
    expect(JSON.stringify(source)).not.toContain('must-not-be-copied');
    expect(source.notes.join('')).toContain('不是 HTTP 请求数');
    expect(source.notes.join('')).toContain('不能将这些耗时相加');
    expect(source.notes.join('')).toContain('旧实现还可能包含额外元信息查询');
    expect(source.notes.join('')).toContain('incompleteHistory');
  });

  it.each([[0, '0 毫秒'], [999, '999 毫秒'], [1000, '1.0 秒'], [59999, '60.0 秒'],
    [60000, '1 分 0 秒'], [120345, '2 分 0 秒'], [NaN, '—'], [-1, '—']])('formats %s without estimated-time wording', (ms, expected) => {
    expect(reportDuration(ms)).toBe(expected);
  });

  it('distinguishes failed, paused and awaiting-confirmation reports from completion', () => {
    expect(keyChangeReportOutcome('completed')).toBe('改密已完成');
    expect(keyChangeReportOutcome('paused')).toBe('已暂停');
    expect(keyChangeReportOutcome('failed')).toContain('失败');
    expect(keyChangeReportOutcome('awaitingConfirmation')).toContain('等待');
    expect(keyChangeReportOutcome('confirmationFailed')).toContain('未完成');
    expect(keyChangeReportOutcome('unknown')).toBe('改密统计');
  });
});

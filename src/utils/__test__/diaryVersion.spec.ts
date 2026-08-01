import {describe, expect, it} from 'vitest';
import type {DiaryVersionReport} from '../../bindings';
import {
  initialDiaryVersionDisplay,
  isDiaryVersionReportCurrent,
  reduceDiaryVersionEvent,
  withDiaryVersionError,
} from '../diaryVersion';

function report(overrides: Partial<DiaryVersionReport> = {}): DiaryVersionReport {
  return {
    scope: 'local',
    currentVersion: 4,
    totalDiaries: 3,
    processedDiaries: 3,
    currentDiaries: 3,
    legacyDiaries: 0,
    newerDiaries: 0,
    failedDiaries: 0,
    upgradedDiaries: 0,
    versions: [{version: 4, count: 3}],
    failedDiaryIds: [],
    ...overrides,
  };
}

describe('diary version display', () => {
  it('tracks an inspection from start through progress to completion', () => {
    let state = initialDiaryVersionDisplay();
    state = reduceDiaryVersionEvent(state, {
      event: 'started',
      data: {operation: 'inspect', scope: 'cloud', total: 2},
    });
    state = reduceDiaryVersionEvent(state, {
      event: 'progress',
      data: {
        operation: 'inspect',
        processed: 1,
        total: 2,
        diaryId: '123',
        outcome: 'legacy',
      },
    });

    expect(state).toMatchObject({
      phase: 'running',
      operation: 'inspect',
      scope: 'cloud',
      processed: 1,
      total: 2,
      currentDiaryId: '123',
      currentOutcome: 'legacy',
    });

    const completed = report({scope: 'cloud', totalDiaries: 2, currentDiaries: 1, legacyDiaries: 1});
    state = reduceDiaryVersionEvent(state, {
      event: 'completed',
      data: {operation: 'inspect', report: completed},
    });
    expect(state).toMatchObject({phase: 'completed', report: completed, currentDiaryId: ''});
  });

  it('keeps a partial report when cancellation is acknowledged', () => {
    const partial = report({totalDiaries: 10, processedDiaries: 4, currentDiaries: 4});
    const state = reduceDiaryVersionEvent(initialDiaryVersionDisplay('upgrade'), {
      event: 'cancelled',
      data: {operation: 'upgrade', report: partial},
    });

    expect(state.phase).toBe('cancelled');
    expect(state.processed).toBe(4);
    expect(state.total).toBe(10);
    expect(state.report).toEqual(partial);
  });

  it('only treats a complete, fully current report as ready for compatibility removal', () => {
    expect(isDiaryVersionReportCurrent(report())).toBe(true);
    expect(isDiaryVersionReportCurrent(report({processedDiaries: 2}))).toBe(false);
    expect(isDiaryVersionReportCurrent(report({currentDiaries: 2, legacyDiaries: 1}))).toBe(false);
    expect(isDiaryVersionReportCurrent(report({currentDiaries: 2, newerDiaries: 1}))).toBe(false);
    expect(isDiaryVersionReportCurrent(report({currentDiaries: 2, failedDiaries: 1}))).toBe(false);
  });

  it('applies command-level errors', () => {
    const failed = withDiaryVersionError(initialDiaryVersionDisplay('inspect'), '读取失败');
    expect(failed).toMatchObject({phase: 'failed', operation: 'inspect', error: '读取失败'});
  });
});

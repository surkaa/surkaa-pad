import {describe, expect, it} from 'vitest';
import type {KeyChangeFileProgress, KeyChangeProgress} from '../../bindings';
import {KeyChangeTimeEstimator, keyChangeEstimatedCompletion} from '../keyChangeTimeEstimate';

function progress(completedBytes: number, extra: Partial<KeyChangeProgress> = {}): KeyChangeProgress {
  return {phase: 'encrypting', completedFiles: 0, totalFiles: 100,
    completedBytes, totalBytes: 1000, currentFile: 'object', fileProgress: null, ...extra};
}

function file(step: KeyChangeFileProgress['step'], points = 0, size = 800): KeyChangeFileProgress {
  return {step, completedBytes: size * points / 10, totalBytes: size, completedPoints: points, totalPoints: 10};
}

function estimator() {
  const value = new KeyChangeTimeEstimator();
  value.reset(0);
  value.update(progress(0));
  return value;
}

describe('byte-based key change time estimate', () => {
  it('waits for byte progress and enough active time instead of guessing from object counts', () => {
    const value = estimator();
    expect(value.remainingMs(10000)).toBeNull();
    value.update(progress(0, {completedFiles: 99}));
    expect(value.remainingMs(10000)).toBeNull();
    value.update(progress(100));
    expect(value.remainingMs(4999)).toBeNull();
    expect(value.remainingMs(10000)).toBe(90000);
  });

  it('gives the same result for the same bytes regardless of file count', () => {
    for (const completedFiles of [1, 70, 99]) {
      const value = estimator();
      value.update(progress(200, {completedFiles, totalFiles: 999}));
      expect(value.remainingMs(10000)).toBe(40000);
    }
  });

  it('uses current large-file steps before the first checkpoint without counting repeated passes twice', () => {
    const value = estimator();
    value.update(progress(0, {fileProgress: file('writingStage')}));
    value.update(progress(0, {fileProgress: file('writingStage', 5)}));
    expect(value.remainingMs(10000)).toBe(190000); // 800 × 0.5 / 8 = 50 原数据字节。
    value.update(progress(0, {fileProgress: file('writingStage', 5)}));
    expect(value.remainingMs(10000)).toBe(190000);
    value.update(progress(0, {fileProgress: file('checkingStageContent')}));
    expect(value.remainingMs(10000)).toBe(90000);
    value.update(progress(0));
    expect(value.remainingMs(10000)).toBe(90000); // 清空单文件 UI 不丢失测速进度。
    value.update(progress(0, {fileProgress: file('checkingStageContent', 10)}));
    expect(value.remainingMs(10000)).toBe(40000);
  });

  it('moves from final file steps to saved checkpoint bytes without double counting', () => {
    const value = estimator();
    value.update(progress(0, {fileProgress: file('writingStage')}));
    value.update(progress(0, {fileProgress: file('checkingCommitted', 10)}));
    expect(value.remainingMs(10000)).toBe(2500);
    value.update(progress(800, {currentFile: 'next-object'}));
    expect(value.remainingMs(10000)).toBe(2500);
    value.update(progress(800, {currentFile: 'next-object', fileProgress: file('writingStage', 5, 200)}));
    expect(value.remainingMs(10000)).toBeCloseTo(187.5 / 812.5 * 10000);
  });

  it('resets baseline and elapsed time after a pause or restart', () => {
    const value = estimator();
    value.update(progress(100));
    expect(value.remainingMs(10000)).toBe(90000);
    value.reset(1000000);
    value.update(progress(800));
    expect(value.remainingMs(1010000)).toBeNull();
    value.update(progress(900));
    expect(value.remainingMs(1010000)).toBe(10000);
  });

  it('excludes already-completed steps when continuing from verified staged data', () => {
    const value = estimator();
    value.update(progress(0, {fileProgress: file('checkingTarget')}));
    expect(value.remainingMs(10000)).toBeNull();
    value.update(progress(0, {fileProgress: file('checkingTarget', 5)}));
    expect(value.remainingMs(10000)).toBe(150000); // 既有 200 字节不计入本次速度。
  });

  it('includes request waiting in elapsed time and does not pretend a stalled transfer is counting down', () => {
    const value = estimator();
    value.update(progress(100));
    expect(value.remainingMs(10000)).toBe(90000);
    expect(value.remainingMs(20000)).toBe(180000);
  });

  it('ignores stale or invalid progress and bounds oversized current-file data', () => {
    const value = estimator();
    value.update(progress(100));
    for (const completedBytes of [-1, NaN, 0]) {
      value.update(progress(completedBytes));
      expect(value.remainingMs(10000)).toBe(90000);
    }
    for (const totalBytes of [-1, NaN, Infinity]) {
      value.update(progress(100, {totalBytes}));
      expect(value.remainingMs(10000)).toBe(90000);
    }
    value.update(progress(100, {fileProgress: file('writingStage', 99, 2000)}));
    expect(value.remainingMs(10000)).toBeCloseTo(787.5 / 212.5 * 10000);
    expect(value.remainingMs(NaN)).toBeNull();
    expect(value.remainingMs(-1)).toBeNull();
  });

  it('handles empty data and the final checkpoint without using file counts', () => {
    const value = estimator();
    value.update(progress(1000, {phase: 'switchingKey'}));
    expect(value.remainingMs(10000)).toBe(0);
    value.reset(0);
    value.update(progress(0, {totalBytes: 0, totalFiles: 0}));
    expect(value.remainingMs(10000)).toBeNull();
  });
});

describe('local estimated completion clock', () => {
  it('shows time without UTC and adds the date on the next day or year', () => {
    expect(keyChangeEstimatedCompletion(60000, new Date(2026, 9, 12, 10, 0))).toBe('10:01');
    expect(keyChangeEstimatedCompletion(120000, new Date(2026, 9, 12, 23, 59))).toBe('10/13 00:01');
    expect(keyChangeEstimatedCompletion(120000, new Date(2026, 11, 31, 23, 59))).toBe('2027/1/1 00:01');
  });

  it('does not throw for invalid or unrepresentable times', () => {
    for (const duration of [-1, NaN, Infinity, Number.MAX_VALUE]) {
      expect(keyChangeEstimatedCompletion(duration, new Date())).toBeNull();
    }
    expect(keyChangeEstimatedCompletion(1000, new Date(NaN))).toBeNull();
  });
});

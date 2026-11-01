import {describe, expect, it} from 'vitest';
import {archiveEntryKind, archiveEntryProgressValue} from '../archiveEntryPreview';
import {buildArchiveTree, visibleArchiveNodes} from '../archivePreview';

describe('archive entry preview classification and identity', () => {
  it.each([
    ['image.JPG', '', 'image'], ['image.bin', 'image/png', 'image'], ['recording.m4a', '', 'audio'],
    ['video.mp4', '', 'video'], ['movie.bin', 'video/webm', 'video'], ['document.PDF', '', 'pdf'],
    ['README.md', '', 'markdown'], ['data.json', '', 'json'], ['large.JSONL', '', 'jsonl'],
    ['rows.ndjson', '', 'jsonl'], ['example.rs', '', 'text'], ['page.htm', '', 'html'],
    ['nested.zip', '', null], ['nested.7z', '', null], ['nested.rar', 'text/plain', null],
    ['unknown.bin', 'application/octet-stream', null],
  ])('classifies %s without turning nested archives into text', (filename, mimetype, kind) => {
    expect(archiveEntryKind({filename: filename!, mimetype: mimetype!})).toBe(kind);
  });

  it('preserves original entry indexes after directory synthesis, duplicate names, sorting and filtering', () => {
    const raw = ['file10.txt', 'dir/z.txt', 'dir/z.txt', 'file2.txt'].map((path, index) => ({path,
      isDirectory: false, size: index, compressedSize: index, encrypted: false, modifiedAt: null}));
    const roots = buildArchiveTree(raw);
    const matches = visibleArchiveNodes(roots, new Set(), 'Z.TXT');
    expect(matches.map(item => item.sourceIndex)).toEqual([null, 1, 2]);
    expect(new Set(matches.map(item => item.key)).size).toBe(3);
    expect(roots.slice(1).map(item => item.sourceIndex)).toEqual([3, 0]);
  });
});

describe('archive entry progress', () => {
  it('keeps preparation empty and verification full regardless of their byte counters', () => {
    expect(archiveEntryProgressValue(null)).toBe(0);
    for (const [processedBytes, totalBytes] of [[0, 0], [50, 100], [100, 100], [NaN, Infinity]]) {
      expect(archiveEntryProgressValue({phase: 'preparing', processedBytes, totalBytes})).toBe(0);
      expect(archiveEntryProgressValue({phase: 'verifying', processedBytes, totalBytes})).toBe(1);
    }
  });
  it('keeps extraction empty until a valid byte total is known, including empty files', () => {
    for (const totalBytes of [0, -1, NaN, Infinity]) expect(archiveEntryProgressValue({phase: 'extracting', processedBytes: 1, totalBytes})).toBe(0);
  });
  it('uses byte fractions and clamps invalid endpoints', () => {
    const progress = {phase: 'extracting' as const, totalBytes: 100, processedBytes: 25};
    expect(archiveEntryProgressValue(progress)).toBe(.25);
    expect(archiveEntryProgressValue({...progress, processedBytes: -10})).toBe(0);
    expect(archiveEntryProgressValue({...progress, processedBytes: 200})).toBe(1);
    expect(archiveEntryProgressValue({...progress, processedBytes: NaN})).toBe(0);
  });
});

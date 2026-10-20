import {afterEach, describe, expect, it, vi} from 'vitest';
import {fetchJsonlPage, MAX_JSONL_LINE_BYTES, parseJsonlLine, type JsonlLine} from '../jsonlPreview';

const line = (text = '{}', lineNumber = 1): JsonlLine => ({lineNumber, text, error: null});
afterEach(() => vi.unstubAllGlobals());

describe('JSONL page loading', () => {
  it('keeps the attachment token and cursor while using the paginated view', async () => {
    const page = {lines: [line()], nextCursor: 'next-cursor'};
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(page)));
    vi.stubGlobal('fetch', fetchMock);
    const controller = new AbortController();
    await expect(fetchJsonlPage('http://127.0.0.1:123/token/diary/file?t=1&view=pdf&cursor=old', 'current', controller.signal)).resolves.toEqual(page);
    const [rawUrl, options] = fetchMock.mock.calls[0];
    const url = new URL(rawUrl);
    expect(url.pathname).toBe('/token/diary/file');
    expect(url.searchParams.get('t')).toBe('1');
    expect(url.searchParams.get('view')).toBe('jsonl');
    expect(url.searchParams.get('cursor')).toBe('current');
    expect(options).toEqual({cache: 'no-store', signal: controller.signal});
  });

  it('omits the cursor on the first page and accepts the end of an empty file', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response('{"lines":[],"nextCursor":null}'));
    vi.stubGlobal('fetch', fetchMock);
    await expect(fetchJsonlPage('http://127.0.0.1/token/file?cursor=old', null)).resolves.toEqual({lines: [], nextCursor: null});
    expect(new URL(fetchMock.mock.calls[0][0]).searchParams.has('cursor')).toBe(false);
  });

  it('rejects malformed, oversized, nonconsecutive and nonadvancing pages', async () => {
    for (const value of [null, {}, {lines: [], nextCursor: 'next'}, {lines: [line()]},
      {lines: [line()], nextCursor: 'current'}, {lines: Array.from({length: 11}, (_, i) => line('{}', i + 1)), nextCursor: null},
      {lines: [line('{}', 0)], nextCursor: null}, {lines: [line('{}', 1), line('{}', 3)], nextCursor: null},
      {lines: [line('{}', Number.MAX_SAFE_INTEGER + 1)], nextCursor: null},
      {lines: [{lineNumber: 1, text: '{}'}], nextCursor: null},
      {lines: [line('a'.repeat(MAX_JSONL_LINE_BYTES + 1))], nextCursor: null}]) {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(value))));
      await expect(fetchJsonlPage('http://127.0.0.1/file', 'current')).rejects.toThrow('分页响应无效');
    }
  });

  it('shows the backend line-limit error and bounds oversized response bodies', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('JSONL 单行超过 4 MiB 安全上限', {status: 400})));
    await expect(fetchJsonlPage('http://127.0.0.1/file', null)).rejects.toThrow('单行超过 4 MiB');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', {headers: {'content-length': String(32 * 1024 * 1024 + 1)}})));
    await expect(fetchJsonlPage('http://127.0.0.1/file', null)).rejects.toThrow('超过');
  });

  it.each([1024 * 1024 + 1, 4 * 1024 * 1024])('accepts a complete %s-byte JSON line without truncation', async size => {
    const text = `"${'a'.repeat(size - 2)}"`;
    const page = {lines: [line(text)], nextCursor: 'next'};
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(page))));
    const result = await fetchJsonlPage('http://127.0.0.1/file', null);
    expect(result.lines[0].text).toBe(text);
    expect(result.nextCursor).toBe('next');
  });
});

describe('JSONL line display', () => {
  it.each(['{}', '[1,2]', 'null', 'false', '0', '"中文"'])('parses any valid JSON value: %s', text => {
    expect(parseJsonlLine(line(text))).toEqual({value: JSON.parse(text), treeSafe: true, error: ''});
  });
  it('keeps invalid JSON, blank lines and decoding errors separate from valid records', () => {
    expect(parseJsonlLine(line('{bad}')).error).toContain('JSON 格式错误');
    expect(parseJsonlLine(line(' ')).error).toContain('空行');
    expect(parseJsonlLine({...line(), error: 'UTF-8 错误'}).error).toBe('UTF-8 错误');
    expect(parseJsonlLine(line('{"ok":true}')).treeSafe).toBe(true);
  });
  it('falls back to plain text for deeply nested or very wide JSON trees', () => {
    expect(parseJsonlLine(line('['.repeat(70) + '1' + ']'.repeat(70))).treeSafe).toBe(false);
    const wide = JSON.stringify(Array.from({length: 10_001}, () => 1));
    expect(parseJsonlLine(line(wide)).error).toContain('结构较大或过深');
  });
});

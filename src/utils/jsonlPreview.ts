import {readBoundedResponseBytes} from './attachmentPreview';

export interface JsonlLine {
  lineNumber: number;
  text: string;
  error: string | null;
}

export interface JsonlPage {
  lines: JsonlLine[];
  nextCursor: string | null;
}

export const JSONL_ROW_MIN_HEIGHT = 104;
export const MAX_JSONL_LINE_BYTES = 4 * 1024 * 1024;

// 后端每页原文最多 4 MiB；JSON 转义最多放大六倍，仍对响应设置硬上限。
const MAX_PAGE_RESPONSE_BYTES = 32 * 1024 * 1024;

export async function fetchJsonlPage(
  attachmentUrl: string,
  cursor: string | null,
  signal?: AbortSignal,
): Promise<JsonlPage> {
  const url = new URL(attachmentUrl);
  url.searchParams.set('view', 'jsonl');
  url.searchParams.delete('cursor');
  if (cursor !== null) url.searchParams.set('cursor', cursor);
  const response = await fetch(url.toString(), {signal, cache: 'no-store'});
  if (!response.ok) {
    let detail = '';
    try {
      detail = new TextDecoder().decode(await readBoundedResponseBytes(response, 4096)).trim();
    } catch { /* 错误响应也不能无上限读取。 */ }
    throw new Error(detail || `读取 JSONL 失败（HTTP ${response.status}）`);
  }
  const value: unknown = JSON.parse(new TextDecoder('utf-8', {fatal: true})
    .decode(await readBoundedResponseBytes(response, MAX_PAGE_RESPONSE_BYTES)));
  if (!value || typeof value !== 'object') throw invalidPage();
  const page = value as Partial<JsonlPage>;
  if (!Array.isArray(page.lines) || page.lines.length > 10
    || (page.nextCursor !== null && (typeof page.nextCursor !== 'string'
      || !page.nextCursor || page.nextCursor.length > 4096 || page.nextCursor === cursor))) throw invalidPage();
  let previous = 0;
  for (const line of page.lines) {
    if (!line || !Number.isSafeInteger(line.lineNumber) || line.lineNumber < 1
      || (previous !== 0 && line.lineNumber !== previous + 1)
      || typeof line.text !== 'string' || line.text.length > MAX_JSONL_LINE_BYTES
      || (line.error !== null && typeof line.error !== 'string')) throw invalidPage();
    previous = line.lineNumber;
  }
  if (page.lines.length === 0 && page.nextCursor !== null) throw invalidPage();
  return {lines: page.lines, nextCursor: page.nextCursor};
}

function invalidPage(): Error { return new Error('JSONL 分页响应无效，请重新打开预览'); }

export function parseJsonlLine(line: JsonlLine): {value?: unknown; error: string; treeSafe: boolean} {
  if (line.error) return {error: line.error, treeSafe: false};
  if (!line.text.trim()) return {error: '空行不是有效的 JSON，已保留原始行号', treeSafe: false};
  try {
    const value: unknown = JSON.parse(line.text);
    const stack = [{value, depth: 0}];
    let nodes = 0;
    while (stack.length) {
      const item = stack.pop()!;
      if (++nodes > 10_000 || item.depth > 64) {
        return {value, error: '该行 JSON 结构较大或过深，展开时按原文显示', treeSafe: false};
      }
      if (item.value && typeof item.value === 'object') {
        for (const child of Object.values(item.value)) stack.push({value: child, depth: item.depth + 1});
      }
    }
    return {value, error: '', treeSafe: true};
  } catch (error) {
    return {error: `JSON 格式错误：${error instanceof Error ? error.message : String(error)}`, treeSafe: false};
  }
}

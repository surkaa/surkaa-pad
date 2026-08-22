import {describe, expect, it} from 'vitest';
import {
  containsNestedJsonObjectString,
  expandNestedJsonObjectStrings,
  formatJsonSource,
} from '../jsonSource';

describe('JSON source formatting', () => {
  it('formats the source without changing it by default', () => {
    const source = {model: 'qwen3:8b', messages: [{role: 'user', content: '你好'}]};

    expect(formatJsonSource(source)).toBe(JSON.stringify(source, null, 2));
  });

  it('recursively expands only strings containing a complete JSON object', () => {
    const source = {
      arguments: '{"diaryId":"123"}',
      result: '{"ok":true,"nested":"{\\"value\\":1}"}',
      array: '[1,2]',
      incomplete: '{"value":',
      text: '普通文本',
    };

    expect(expandNestedJsonObjectStrings(source)).toEqual({
      arguments: {diaryId: '123'},
      result: {ok: true, nested: {value: 1}},
      array: '[1,2]',
      incomplete: '{"value":',
      text: '普通文本',
    });
    expect(source.arguments).toBe('{"diaryId":"123"}');
  });

  it('detects whether the source contains an expandable object string', () => {
    expect(containsNestedJsonObjectString({content: '{"ok":true}'})).toBe(true);
    expect(containsNestedJsonObjectString({content: '[1,2]'})).toBe(false);
    expect(containsNestedJsonObjectString({content: '普通文本'})).toBe(false);
  });
});

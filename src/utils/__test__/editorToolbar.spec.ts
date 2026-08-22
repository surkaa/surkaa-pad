import {describe, expect, it, vi} from 'vitest';
import type {Editor} from '@tiptap/vue-3';
import {
  DEFAULT_EDITOR_TOOLBAR_ORDER,
  moveEditorToolbarAction,
  normalizeEditorToolbarOrder,
  runEditorToolbarAction,
} from '../editorToolbar';

describe('editor toolbar order', () => {
  it('uses the default order for missing or invalid configuration', () => {
    expect(normalizeEditorToolbarOrder(undefined)).toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(normalizeEditorToolbarOrder('bold')).toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
  });

  it('keeps valid unique actions and appends missing actions', () => {
    expect(normalizeEditorToolbarOrder(['taskList', 'bold', 'taskList', 'unknown'])).toEqual([
      'taskList',
      'bold',
      'underline',
      'strike',
      'heading1',
      'heading2',
      'heading3',
      'summary',
    ]);
  });

  it('moves one action without crossing the order boundaries', () => {
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'underline', -1).slice(0, 3))
      .toEqual(['underline', 'bold', 'strike']);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'bold', -1))
      .toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'taskList', 1))
      .not.toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'summary', 1))
      .toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
  });

  it('runs formatting commands and delegates Summary editing', () => {
    const run = vi.fn(() => true);
    const chain: Record<string, any> = {run};
    for (const method of [
      'focus',
      'toggleBold',
      'toggleUnderline',
      'toggleStrike',
      'toggleHeading',
      'toggleTaskList',
    ]) {
      chain[method] = vi.fn(() => chain);
    }
    const editor = {chain: vi.fn(() => chain)} as unknown as Editor;
    const openSummary = vi.fn();

    expect(runEditorToolbarAction(editor, 'heading2', openSummary)).toBe(true);
    expect(chain.toggleHeading).toHaveBeenCalledWith({level: 2});
    expect(openSummary).not.toHaveBeenCalled();

    expect(runEditorToolbarAction(editor, 'summary', openSummary)).toBe(true);
    expect(openSummary).toHaveBeenCalledOnce();
  });
});

import {describe, expect, it, vi} from 'vitest';
import type {Editor} from '@tiptap/vue-3';
import {
  DEFAULT_EDITOR_TOOLBAR_ORDER,
  moveEditorToolbarAction,
  normalizeEditorToolbarOrder,
  reorderEditorToolbarAction,
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
      'heading4',
      'heading5',
      'summary',
      'undo',
      'redo',
    ]);
  });

  it('inserts new heading actions after heading3 in an old complete order', () => {
    expect(normalizeEditorToolbarOrder([
      'redo',
      'heading3',
      'bold',
      'taskList',
      'heading2',
      'heading1',
      'underline',
      'summary',
      'strike',
      'undo',
    ])).toEqual([
      'redo',
      'heading3',
      'heading4',
      'heading5',
      'bold',
      'taskList',
      'heading2',
      'heading1',
      'underline',
      'summary',
      'strike',
      'undo',
    ]);
  });

  it('moves one action without crossing the order boundaries', () => {
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'underline', -1).slice(0, 3))
      .toEqual(['underline', 'bold', 'strike']);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'bold', -1))
      .toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'taskList', 1))
      .not.toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'redo', 1))
      .toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(moveEditorToolbarAction(DEFAULT_EDITOR_TOOLBAR_ORDER, 'undo', -1).slice(-4))
      .toEqual(['taskList', 'undo', 'summary', 'redo']);
  });

  it('reorders an action at a stable insertion index', () => {
    expect(reorderEditorToolbarAction(
      DEFAULT_EDITOR_TOOLBAR_ORDER,
      'bold',
      5,
    ).slice(0, 7)).toEqual([
      'underline',
      'strike',
      'heading1',
      'heading2',
      'heading3',
      'bold',
      'heading4',
    ]);
    expect(reorderEditorToolbarAction(
      DEFAULT_EDITOR_TOOLBAR_ORDER,
      'heading3',
      5,
    )).toEqual(DEFAULT_EDITOR_TOOLBAR_ORDER);
    expect(reorderEditorToolbarAction(
      DEFAULT_EDITOR_TOOLBAR_ORDER,
      'redo',
      -10,
    )[0]).toBe('redo');
    const movedToEnd = reorderEditorToolbarAction(
      DEFAULT_EDITOR_TOOLBAR_ORDER,
      'bold',
      999,
    );
    expect(movedToEnd[movedToEnd.length - 1]).toBe('bold');
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
      'undo',
      'redo',
    ]) {
      chain[method] = vi.fn(() => chain);
    }
    const editor = {chain: vi.fn(() => chain)} as unknown as Editor;
    const openSummary = vi.fn();

    expect(runEditorToolbarAction(editor, 'heading5', openSummary)).toBe(true);
    expect(chain.toggleHeading).toHaveBeenCalledWith({level: 5});
    expect(openSummary).not.toHaveBeenCalled();

    expect(runEditorToolbarAction(editor, 'summary', openSummary)).toBe(true);
    expect(openSummary).toHaveBeenCalledOnce();

    expect(runEditorToolbarAction(editor, 'undo', openSummary)).toBe(true);
    expect(chain.undo).toHaveBeenCalledOnce();
  });
});

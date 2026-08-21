// @vitest-environment happy-dom
import {describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {SummaryNode} from '../tiptap-extensions/SummaryNode';

describe('SummaryNode', () => {
  it('inserts a structured native details element', () => {
    const element = document.createElement('div');
    document.body.appendChild(element);
    const editor = new Editor({
      element,
      extensions: [StarterKit, SummaryNode],
      content: '<p></p>',
    });

    expect(editor.commands.insertSummary({
      summary: '外显文字',
      content: '第一行\n第二行',
    })).toBe(true);
    expect(editor.getJSON().content).toContainEqual(expect.objectContaining({
      type: 'summaryNode',
      attrs: {summary: '外显文字', content: '第一行\n第二行'},
    }));
    expect(element.querySelector('details.editor-summary > summary > span')?.textContent)
      .toBe('外显文字');
    expect(element.querySelector('.editor-summary-content')?.textContent)
      .toBe('第一行\n第二行');

    editor.destroy();
    element.remove();
  });

  it('reports the current position and attributes from its edit button', () => {
    const element = document.createElement('div');
    document.body.appendChild(element);
    const onEdit = vi.fn();
    const editor = new Editor({
      element,
      extensions: [StarterKit, SummaryNode.configure({onEdit})],
      content: '<details class="editor-summary" data-summary="摘要" data-content="正文"></details>',
    });

    (element.querySelector('.editor-summary-edit') as HTMLButtonElement).click();

    expect(onEdit).toHaveBeenCalledOnce();
    expect(onEdit.mock.calls[0][0]).toEqual(expect.any(Number));
    expect(onEdit.mock.calls[0][1]).toEqual({summary: '摘要', content: '正文'});

    editor.destroy();
    element.remove();
  });

  it('toggles the native details state when motion animation is unavailable', () => {
    const element = document.createElement('div');
    document.body.appendChild(element);
    const editor = new Editor({
      element,
      extensions: [StarterKit, SummaryNode],
      content: '<details class="editor-summary" data-summary="摘要" data-content="正文"></details>',
    });
    const details = element.querySelector('details.editor-summary') as HTMLDetailsElement;
    const summary = details.querySelector('summary') as HTMLElement;

    summary.click();
    expect(details.open).toBe(true);
    summary.click();
    expect(details.open).toBe(false);

    editor.destroy();
    element.remove();
  });
});

// @vitest-environment happy-dom
import {describe, expect, it, vi} from 'vitest';
import {Editor} from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import {SummaryNode} from '../tiptap-extensions/SummaryNode';
import {openUrl} from '@tauri-apps/plugin-opener';
import {diaryContentToHtml, htmlToDiaryContent} from '../markdownConverter';
vi.mock('@tauri-apps/plugin-opener', () => ({openUrl: vi.fn().mockResolvedValue(undefined)}));

describe('SummaryNode', () => {
  it('renders Markdown blocks while persisting the exact original source across save/reopen', () => {
    const markdown = '# 标题\n\n**加粗**与`代码`\n\n> 引用\n\n- 一\n  - 二\n\n- [x] 完成\n\n'
      + '| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```ts\nconst x = "<b>";\n```';
    const original = {nodes: [{type: 'summary' as const, summary: '摘要 <标题>', content: markdown}]};
    const element = document.createElement('div'); document.body.append(element);
    const editor = new Editor({element, extensions: [StarterKit, SummaryNode], content: diaryContentToHtml(original, {})});
    try {
      expect(element.querySelector('.editor-summary-content h1')?.textContent).toBe('标题');
      expect(element.querySelector('.editor-summary-content strong')?.textContent).toBe('加粗');
      expect(element.querySelector('.editor-summary-content blockquote')).not.toBeNull();
      expect(element.querySelectorAll('.editor-summary-content ul ul li')).toHaveLength(1);
      expect(element.querySelector('.editor-summary-content li[data-checked="true"]')).not.toBeNull();
      expect(element.querySelector('.editor-summary-content table td')?.textContent).toBe('1');
      expect(element.querySelector('.editor-summary-content pre code')?.textContent).toBe('const x = "<b>";');
      const before = editor.getJSON();
      element.querySelector<HTMLElement>('summary')!.click();
      expect(element.querySelector<HTMLDetailsElement>('details')?.open).toBe(true);
      expect(editor.getJSON()).toEqual(before); // 展开状态不是日记内容编辑。
      const html = editor.getHTML();
      expect(htmlToDiaryContent(html)).toEqual(original);
      editor.commands.setContent(html);
      expect(htmlToDiaryContent(editor.getHTML())).toEqual(original);
    } finally { editor.destroy(); element.remove(); }
  });

  it('does not execute HTML, create nested summaries, auto-load images or allow script links', () => {
    const source = '<script>alert(1)</script>\n\n<details><summary>嵌套</summary>隐藏</details>\n\n'
      + '![远程图片](https://example.com/private.png)\n\n[危险](javascript:alert(1))';
    const element = document.createElement('div'); document.body.append(element);
    const editor = new Editor({element, extensions: [StarterKit, SummaryNode], content: diaryContentToHtml({nodes: [
      {type: 'summary', summary: '安全渲染', content: source},
    ]}, {})});
    try {
      expect(element.querySelectorAll('details')).toHaveLength(1);
      expect(element.querySelector('.editor-summary-content script')).toBeNull();
      expect(element.querySelector('.editor-summary-content img')).toBeNull();
      expect(element.querySelector('.editor-summary-content a')).toBeNull();
      expect(element.querySelector('.editor-summary-content')?.textContent).toContain('<script>alert(1)</script>');
      expect(htmlToDiaryContent(editor.getHTML()).nodes[0]).toEqual({type: 'summary', summary: '安全渲染', content: source});
    } finally { editor.destroy(); element.remove(); }
  });

  it('opens safe links only after a user click and prevents navigation inside the application', async () => {
    vi.mocked(openUrl).mockClear();
    const element = document.createElement('div'); document.body.append(element);
    const editor = new Editor({element, extensions: [StarterKit, SummaryNode], content: diaryContentToHtml({nodes: [
      {type: 'summary', summary: '链接', content: '[访问](https://example.com/path)'},
    ]}, {})});
    try {
      expect(openUrl).not.toHaveBeenCalled();
      const anchor = element.querySelector<HTMLAnchorElement>('.editor-summary-content a')!;
      expect(anchor.rel).toBe('noopener noreferrer');
      const event = new MouseEvent('click', {bubbles: true, cancelable: true}); anchor.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
      expect(openUrl).toHaveBeenCalledExactlyOnceWith('https://example.com/path');
    } finally { editor.destroy(); element.remove(); }
  });

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
    expect(element.querySelector('.editor-summary-content p')?.innerHTML).toBe('第一行<br>第二行');
    expect(element.querySelector<HTMLElement>('details')?.dataset.content).toBe('第一行\n第二行');
    const details = element.querySelector('details.editor-summary') as HTMLDetailsElement;
    const content = details.querySelector('.editor-summary-content') as HTMLElement;
    content.dispatchEvent(new MouseEvent('pointerdown', {bubbles: true}));
    expect(details.draggable).toBe(false);
    (details.querySelector('summary') as HTMLElement)
      .dispatchEvent(new MouseEvent('pointerdown', {bubbles: true}));
    expect(details.draggable).toBe(true);

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

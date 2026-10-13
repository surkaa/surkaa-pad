import {DOMParser as ProseMirrorDOMParser} from '@tiptap/pm/model';
import type {EditorView} from '@tiptap/pm/view';
import {renderPastedEditorMarkdown} from './markdownParser';

/** 保留原始 Markdown 缩进，避免普通文本粘贴把多级列表变成同一级。 */
export function pasteEditorMarkdown(view: EditorView, event: ClipboardEvent): boolean {
  if (!view.editable || !event.clipboardData || event.clipboardData.files?.length) return false;
  const {selection} = view.state;
  for (let depth = selection.$from.depth; depth > 0; depth--) {
    const node = selection.$from.node(depth);
    if (node.type.name === 'table' || node.type.spec.code) return false;
  }
  const html = renderPastedEditorMarkdown(event.clipboardData.getData('text/plain'));
  if (!html) return false;
  const container = document.createElement('div');
  container.innerHTML = html;
  const slice = ProseMirrorDOMParser.fromSchema(view.state.schema).parseSlice(container);
  event.preventDefault();
  view.dispatch(view.state.tr.replaceSelection(slice).scrollIntoView());
  return true;
}

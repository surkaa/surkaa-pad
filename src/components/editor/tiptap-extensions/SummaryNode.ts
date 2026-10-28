import {Node as TiptapNode, mergeAttributes} from '@tiptap/vue-3';
import {Fragment} from '@tiptap/pm/model';
import {Selection, TextSelection, NodeSelection, type EditorState} from '@tiptap/pm/state';
import {closeHistory} from '@tiptap/pm/history';

export function summaryRange(state: EditorState) {
  const {selection} = state;
  if (selection instanceof NodeSelection && selection.node.type.name === 'summaryNode') {
    return {node: selection.node, pos: selection.from};
  }
  for (let depth = selection.$from.depth; depth > 0; depth--) {
    if (selection.$from.node(depth).type.name === 'summaryNode') {
      return {node: selection.$from.node(depth), pos: selection.$from.before(depth)};
    }
  }
  return null;
}

const SummaryTitle = TiptapNode.create({
  name: 'summaryTitle', content: 'text*', marks: '', defining: true, selectable: false,
  parseHTML: () => [{tag: '[data-summary-title]'}],
  renderHTML: ({HTMLAttributes}) => ['div', mergeAttributes(HTMLAttributes, {'data-summary-title': '', class: 'editor-summary-title'}), 0],
});
const SummaryBody = TiptapNode.create({
  name: 'summaryBody', content: '(block|diaryBlock)+', isolating: true, defining: true, selectable: false,
  parseHTML: () => [{tag: '[data-summary-body]'}],
  renderHTML: ({HTMLAttributes}) => ['div', mergeAttributes(HTMLAttributes, {'data-summary-body': '', class: 'editor-summary-content'}), 0],
});

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    summaryNode: {
      insertSummary: () => ReturnType;
      exitSummary: () => ReturnType;
      unwrapSummary: () => ReturnType;
      appendToSummary: (position: number) => ReturnType;
      enterSummaryBody: () => ReturnType;
    };
  }
}

/** 标题和正文均由当前 ProseMirror 管理，不是预览 HTML 或第二个编辑器。 */
export const SummaryNode = TiptapNode.create({
  name: 'summaryNode', group: 'summaryBlock', content: 'summaryTitle summaryBody',
  isolating: true, defining: true, draggable: true,
  addExtensions() { return [SummaryTitle, SummaryBody]; },
  parseHTML: () => [{tag: 'section.editor-summary'}],
  renderHTML: ({HTMLAttributes}) => ['section', mergeAttributes(HTMLAttributes, {class: 'editor-summary'}), 0],
  addNodeView() {
    return ({node, getPos, editor}) => {
      let current = node;
      let open = true;
      let animation: Animation | null = null;
      const dom = document.createElement('section'); dom.className = 'editor-summary';
      const contentDOM = document.createElement('div'); contentDOM.className = 'editor-summary-nodes';
      const toggle = document.createElement('button');
      toggle.type = 'button'; toggle.className = 'editor-summary-toggle'; toggle.contentEditable = 'false';
      const footer = document.createElement('div'); footer.className = 'editor-summary-actions'; footer.contentEditable = 'false';
      const exit = document.createElement('button'); exit.type = 'button'; exit.textContent = '在下方继续正文';
      const unwrap = document.createElement('button'); unwrap.type = 'button'; unwrap.textContent = '取消折叠，保留内容';
      footer.append(exit, unwrap); dom.append(toggle, contentDOM, footer);
      const sync = () => {
        dom.classList.toggle('is-collapsed', !open);
        toggle.textContent = open ? '▾' : '▸';
        toggle.setAttribute('aria-label', open ? '收起折叠内容' : '展开折叠内容');
        toggle.setAttribute('aria-expanded', String(open));
      };
      const preventFocus = (event: Event) => event.preventDefault();
      toggle.addEventListener('mousedown', preventFocus);
      footer.addEventListener('mousedown', preventFocus);
      toggle.onclick = () => {
        const pos = getPos();
        if (open && typeof pos === 'number' && editor.state.selection.from > pos + current.firstChild!.nodeSize + 1
          && editor.state.selection.to < pos + current.nodeSize) {
          editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, pos + 2)));
        }
        animation?.cancel(); animation = null;
        open = !open;
        const body = contentDOM.querySelector<HTMLElement>('[data-summary-body]');
        if (!body || typeof body.animate !== 'function' || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
          sync(); return;
        }
        if (open) sync();
        const expanded = {height: `${body.scrollHeight}px`, opacity: 1, overflow: 'hidden'};
        const collapsed = {height: '0px', opacity: 0, overflow: 'hidden'};
        animation = body.animate(open ? [collapsed, expanded] : [expanded, collapsed], {duration: 160, easing: 'ease-out'});
        animation.onfinish = () => {animation = null; sync();};
      };
      const atSummary = () => {
        const pos = getPos();
        if (typeof pos !== 'number' || !editor.isEditable) return false;
        editor.commands.setTextSelection(pos + 2);
        return true;
      };
      exit.onclick = () => { if (atSummary()) editor.chain().exitSummary().focus().run(); };
      unwrap.onclick = () => { if (atSummary()) editor.chain().unwrapSummary().focus().run(); };
      sync();
      return {
        dom, contentDOM,
        update(next) { if (next.type !== current.type) return false; current = next; return true; },
        stopEvent(event) { return event.target instanceof window.Node && (toggle.contains(event.target) || footer.contains(event.target)); },
        ignoreMutation(mutation) {
          return mutation.type !== 'selection' && (mutation.target === dom || toggle.contains(mutation.target) || footer.contains(mutation.target));
        },
        destroy() {
          animation?.cancel(); toggle.removeEventListener('mousedown', preventFocus);
          footer.removeEventListener('mousedown', preventFocus); toggle.onclick = null; exit.onclick = null; unwrap.onclick = null;
        },
      };
    };
  },
  addCommands() {
    return {
      enterSummaryBody: () => ({state, tr, dispatch}) => {
        const target = summaryRange(state); if (!target) return false;
        const start = target.pos + target.node.firstChild!.nodeSize + 2;
        if (dispatch) {
          if (!target.node.lastChild!.firstChild?.isTextblock) tr.insert(start, state.schema.nodes.paragraph.create());
          tr.setSelection(TextSelection.create(tr.doc, start + 1)).scrollIntoView();
        }
        return true;
      },
      appendToSummary: position => ({state, tr, dispatch}) => {
        if (!this.editor.isEditable || state.selection.empty || summaryRange(state)) return false;
        const target = state.doc.nodeAt(position);
        if (target?.type !== this.type || (state.selection.from < position + target.nodeSize && state.selection.to > position)) return false;
        let content = state.doc.slice(state.selection.from, state.selection.to).content;
        if (content.firstChild?.isInline) content = Fragment.from(state.schema.nodes.paragraph.create(null, content));
        if (!state.schema.nodes.summaryBody.validContent(content)) return false;
        let nested = false; content.descendants(node => {if (node.type === this.type) nested = true;});
        if (nested) return false;
        if (dispatch) {
          closeHistory(tr);
          tr.deleteSelection();
          const mapped = tr.mapping.map(position);
          const updated = tr.doc.nodeAt(mapped)!;
          const end = mapped + updated.nodeSize - 2;
          tr.insert(end, content);
          tr.setSelection(Selection.near(tr.doc.resolve(end), 1)).scrollIntoView();
        }
        return true;
      },
      insertSummary: () => ({state, tr, dispatch}) => {
        if (!this.editor.isEditable) return false;
        if (summaryRange(state)) return false;
        let nested = false;
        state.doc.nodesBetween(state.selection.from, state.selection.to, node => {
          if (node.type.name === 'summaryNode') nested = true;
        });
        if (nested) return false;
        let content = state.selection.empty ? Fragment.from(state.schema.nodes.paragraph.create())
          : state.doc.slice(state.selection.from, state.selection.to).content;
        if (content.firstChild?.isInline) content = Fragment.from(state.schema.nodes.paragraph.create(null, content));
        if (!state.schema.nodes.summaryBody.validContent(content)) return false;
        if (dispatch) {
          closeHistory(tr);
          const title = state.schema.nodes.summaryTitle.create();
          const body = state.schema.nodes.summaryBody.create(null, content);
          tr.replaceSelectionWith(this.type.create(null, [title, body]));
          let position: number | null = null;
          tr.doc.descendants((node, pos) => {
            if (position === null && node.type === this.type && pos >= Math.max(0, state.selection.from - 1)) position = pos;
          });
          if (position !== null) tr.setSelection(TextSelection.create(tr.doc, position + 2));
          tr.scrollIntoView();
        }
        return true;
      },
      exitSummary: () => ({state, tr, dispatch}) => {
        const target = summaryRange(state); if (!target) return false;
        if (dispatch) {
          const end = target.pos + target.node.nodeSize;
          const next = tr.doc.nodeAt(end);
          if (next?.type.name !== 'paragraph' || next.content.size > 0) tr.insert(end, state.schema.nodes.paragraph.create());
          tr.setSelection(TextSelection.create(tr.doc, end + 1)).scrollIntoView();
        }
        return true;
      },
      unwrapSummary: () => ({state, tr, dispatch}) => {
        const target = summaryRange(state); if (!target) return false;
        if (dispatch) {
          closeHistory(tr);
          const title = state.schema.nodes.paragraph.create(null, target.node.firstChild!.content);
          const nodes = [title]; target.node.lastChild!.forEach(node => nodes.push(node));
          tr.replaceWith(target.pos, target.pos + target.node.nodeSize, nodes);
          tr.setSelection(Selection.near(tr.doc.resolve(target.pos + 1))).scrollIntoView();
        }
        return true;
      },
    };
  },
  addKeyboardShortcuts() {
    return {
      Enter: () => {
        const {$from, empty} = this.editor.state.selection;
        const target = summaryRange(this.editor.state);
        if (!empty || !target) return false;
        if ($from.parent.type.name === 'summaryTitle') {
          return this.editor.commands.enterSummaryBody();
        }
        if ($from.parent.type.name === 'paragraph' && !$from.parent.content.size
          && $from.depth > 1 && $from.node($from.depth - 1).type.name === 'summaryBody'
          && $from.index($from.depth - 1) === $from.node($from.depth - 1).childCount - 1) {
          return this.editor.commands.exitSummary();
        }
        return false;
      },
      Backspace: () => {
        const {$from, empty} = this.editor.state.selection;
        return empty && $from.parent.type.name === 'summaryTitle' && $from.parentOffset === 0;
      },
    };
  },
});

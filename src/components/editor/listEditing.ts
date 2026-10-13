import {Extension, type Editor} from '@tiptap/vue-3';

export type EditorListType = 'bulletList' | 'orderedList';
type ListEditor = Pick<Editor, 'isEditable' | 'state' | 'isActive' | 'commands' | 'chain'>;

export function toggleEditorList(editor: ListEditor | null | undefined, type: EditorListType): boolean {
  if (!editor?.isEditable) return false;
  let unsupported = false;
  // 不能把附件等结构化块包入 Markdown 列表而丢失引用。
  const textNodes = new Set(['paragraph', 'heading', 'text', 'hardBreak', 'bulletList', 'orderedList', 'listItem', 'taskList', 'taskItem']);
  editor.state.doc.nodesBetween(editor.state.selection.from, editor.state.selection.to, node => {
    if (!textNodes.has(node.type.name)) unsupported = true;
    return !unsupported;
  });
  if (unsupported || editor.isActive('table')) return false;
  const chain = editor.chain().focus();
  return type === 'bulletList' ? chain.toggleBulletList().run() : chain.toggleOrderedList().run();
}

export function changeEditorListLevel(editor: ListEditor | null | undefined, indent: boolean): boolean {
  if (!editor?.isEditable) return false;
  let itemType: string | undefined;
  for (let depth = editor.state.selection.$from.depth; depth > 0; depth--) {
    const name = editor.state.selection.$from.node(depth).type.name;
    if (name === 'listItem' || name === 'taskItem') { itemType = name; break; }
  }
  if (!itemType) return false;
  return indent ? editor.commands.sinkListItem(itemType) : editor.commands.liftListItem(itemType);
}

export const ListIndentation = Extension.create({
  name: 'diaryListIndentation',
  priority: 1100,
  addKeyboardShortcuts() {
    const change = (indent: boolean) => {
      if (!this.editor.isActive('listItem') && !this.editor.isActive('taskItem')) return false;
      if (this.editor.isActive('table')) return false;
      changeEditorListLevel(this.editor, indent);
      // 首项无法降级时也留在编辑器，不让 Tab 把焦点带到其他控件。
      return true;
    };
    return {Tab: () => change(true), 'Shift-Tab': () => change(false)};
  },
});

import {Extension} from '@tiptap/vue-3';
import {changeEditorListLevel} from '../../utils/editorLists';

export {changeEditorListLevel, toggleEditorList} from '../../utils/editorLists';
export type {EditorListType} from '../../utils/editorLists';

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

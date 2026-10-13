import TaskItem from '@tiptap/extension-task-item';

/** 保留官方 NodeView 的可访问性与更新逻辑，仅替换移动端勾选时的强制 focus。 */
export function createTaskItemExtension(preserveFocusOnCheck: boolean) {
  if (!preserveFocusOnCheck) return TaskItem;
  return TaskItem.extend({
    addNodeView() {
      const parentRenderer = this.parent?.();
      if (!parentRenderer) throw new Error('待办项缺少 NodeView');
      return props => {
        const nodeView = parentRenderer(props);
        const checkbox = (nodeView.dom as HTMLElement)
          .querySelector<HTMLInputElement>(':scope > label > input[type="checkbox"]');
        if (checkbox && this.options.a11y?.checkboxLabel) {
          checkbox.ariaLabel = this.options.a11y.checkboxLabel(props.node, checkbox.checked);
        }
        const onChange = (event: Event) => {
          // 只读模式仍交给官方实现，保留 onReadOnlyChecked 和状态回退。
          if (!props.editor.isEditable || !checkbox) return;
          event.stopImmediatePropagation();
          const position = props.getPos();
          if (typeof position !== 'number') return;
          const {state, view} = props.editor;
          const current = state.doc.nodeAt(position);
          if (current?.type.name !== this.name) return;
          // 不 focus、不 scrollIntoView；选择位置由事务映射，排序插件继续正常执行。
          view.dispatch(state.tr.setNodeMarkup(position, undefined, {
            ...current.attrs, checked: checkbox.checked,
          }));
        };
        checkbox?.addEventListener('change', onChange, true);
        const parentDestroy = nodeView.destroy;
        return {
          ...nodeView,
          destroy: () => {
            checkbox?.removeEventListener('change', onChange, true);
            parentDestroy?.();
          },
        };
      };
    },
  });
}

import {onScopeDispose} from 'vue';
import {useQuasar} from 'quasar';
import {Menu, type MenuOptions} from '@tauri-apps/api/menu';

export interface EditorMenuAction {
  label: string;
  action: () => void | Promise<void>;
  enabled?: boolean;
  danger?: boolean;
}
export type EditorMenuGroup = readonly EditorMenuAction[];

/** 附件和表格共用平台菜单；组间分隔，不替换系统原生菜单的样式。 */
export function useEditorContextMenu(currentPlatform: string) {
  const $q = useQuasar();
  let disposed = false;
  let request = 0;
  let nativeMenu: Menu | undefined;
  let sheet: ReturnType<typeof $q.bottomSheet> | undefined;

  function run(action: EditorMenuAction) {
    if (disposed || action.enabled === false) return;
    void Promise.resolve().then(() => {
      if (!disposed && action.enabled !== false) return action.action();
    }).catch(error => {
      console.error('上下文菜单操作失败:', error);
      $q.notify({type: 'negative', message: '操作失败，请重试'});
    });
  }

  async function closeNativeMenu() {
    const previous = nativeMenu;
    nativeMenu = undefined;
    try { await previous?.close(); }
    catch (error) { console.error('释放上下文菜单失败:', error); }
  }

  async function showMenu(groups: readonly EditorMenuGroup[]) {
    if (disposed) return;
    const currentRequest = ++request;
    const mobile = currentPlatform === 'android' || currentPlatform === 'ios';
    const visibleGroups = groups.map(group => group.filter(action => !mobile || action.enabled !== false))
      .filter(group => group.length > 0);
    if (!visibleGroups.length) return;

    if (mobile) {
      // BottomSheet 不支持禁用单项，隐藏不可执行的操作，并去掉因此产生的空组。
      sheet?.hide();
      const actions = new Map<string, EditorMenuAction>();
      const items: {label?: string; id?: string; style?: {color: string}}[] = [];
      for (const group of visibleGroups) {
        if (items.length) items.push({}); // 无 label 的项由 Quasar 渲染为 QSeparator。
        for (const action of group) {
          const id = String(actions.size);
          actions.set(id, action);
          items.push({label: action.label, id,
            ...(action.danger ? {style: {color: 'var(--pad-danger-color)'}} : {}),
          });
        }
      }
      let selected: EditorMenuAction | undefined;
      const currentSheet = $q.bottomSheet({
        dark: $q.dark.isActive,
        style: {background: 'var(--pad-bg-color-200)', color: 'var(--pad-text-color-100)'},
        actions: items,
      }).onOk((action: {id: string}) => {
        selected = actions.get(action.id);
      }).onDismiss(() => {
        if (sheet === currentSheet) sheet = undefined;
        // 等菜单完全关闭后操作，避免和软键盘、后续弹窗争抢焦点。
        if (selected) run(selected);
      });
      sheet = currentSheet;
      return;
    }

    try {
      await closeNativeMenu();
      if (disposed || currentRequest !== request) return;
      const items: NonNullable<MenuOptions['items']> = [];
      for (const group of visibleGroups) {
        if (items.length) items.push({item: 'Separator'});
        for (const action of group) {
          items.push({text: action.label, enabled: action.enabled !== false, action: () => run(action)});
        }
      }
      // Tauri 的原生菜单不支持逐项文字颜色；删除项保留明确文案和独立分组。
      const menu = await Menu.new({items});
      if (disposed || currentRequest !== request) { await menu.close(); return; }
      nativeMenu = menu;
      await menu.popup(); // 此 Promise 在显示时完成，不能在此处立即释放菜单。
    } catch (error) {
      console.error('上下文菜单失败:', error);
    }
  }

  onScopeDispose(() => {
    disposed = true;
    sheet?.hide();
    void closeNativeMenu();
  });
  return {showMenu};
}

// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, type App} from 'vue';
import {BottomSheet, Dark, Notify, Quasar} from 'quasar';
import type {MenuItemOptions, MenuOptions} from '@tauri-apps/api/menu';
import {useEditorContextMenu, type EditorMenuGroup} from '../useEditorContextMenu';

const native = vi.hoisted(() => ({create: vi.fn()}));
vi.mock('@tauri-apps/api/menu', () => ({Menu: {new: native.create}}));
let app: App | undefined;
let showMenu: ReturnType<typeof useEditorContextMenu>['showMenu'];
let closeMenu: ReturnType<typeof useEditorContextMenu>['closeMenu'];
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(platform: string, dark = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(defineComponent({setup() {
    ({showMenu, closeMenu} = useEditorContextMenu(platform)); return () => h('div');
  }}));
  app.use(Quasar, {plugins: {BottomSheet, Notify}}); Dark.set(dark); app.mount(host); await settle();
}
function items() { return (native.create.mock.lastCall![0] as MenuOptions).items!; }
function sheetItem(label: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.q-bottom-sheet__item'))
    .find(item => item.textContent?.trim() === label)!;
}
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  native.create.mockImplementation(async () => ({popup: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined)}));
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false);
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe('shared editor context menu', () => {
  it('uses native menu separators, disabled states and system colors', async () => {
    await mount('windows');
    const run = vi.fn();
    await showMenu([
      [{label: '复制', action: run}], [],
      [{label: '不可移动', enabled: false, action: run}],
      [{label: '删除', danger: true, action: run}],
    ]);
    expect(items().map(item => 'text' in item ? item.text : 'item' in item ? item.item : undefined))
      .toEqual(['复制', 'Separator', '不可移动', 'Separator', '删除']);
    const disabled = items()[2] as MenuItemOptions;
    expect(disabled.enabled).toBe(false);
    disabled.action!(''); await settle(); expect(run).not.toHaveBeenCalled();
    const deletion = items()[4] as MenuItemOptions;
    expect(deletion).not.toHaveProperty('style');
    expect(deletion).not.toHaveProperty('color');
    deletion.action!(''); await settle(); expect(run).toHaveBeenCalledOnce();
    const menu = await native.create.mock.results[0].value;
    expect(menu.popup).toHaveBeenCalledOnce();
    expect(menu.close).not.toHaveBeenCalled();
    await showMenu([[{label: '复制', action: run}]]);
    expect(menu.close).toHaveBeenCalledOnce();
    const latest = await native.create.mock.results[1].value;
    app!.unmount(); app = undefined; await settle();
    expect(latest.close).toHaveBeenCalledOnce();
    (items()[0] as MenuItemOptions).action!(''); await settle(); expect(run).toHaveBeenCalledOnce();
  });

  it.each([false, true])('uses stock BottomSheet with separators and themed deletion text (dark=%s)', async dark => {
    await mount('android', dark);
    const run = vi.fn();
    const groups: EditorMenuGroup[] = [
      [], [{label: '复制', action: run}],
      [{label: '不可移动', enabled: false, action: run}],
      [{label: '清空', action: run}],
      [{label: '删除', danger: true, action: run}], [],
    ];
    await showMenu(groups); await settle();
    expect(native.create).not.toHaveBeenCalled();
    const sheet = document.querySelector<HTMLElement>('.q-bottom-sheet')!;
    expect(sheet.classList.contains('q-bottom-sheet--dark')).toBe(dark);
    expect(sheet.style.background).toBe('var(--pad-bg-color-200)');
    expect(sheet.style.color).toBe('var(--pad-text-color-100)');
    expect(sheetItem('不可移动')).toBeUndefined();
    expect(sheet.querySelectorAll('.q-separator')).toHaveLength(2);
    expect(sheet.firstElementChild?.classList.contains('q-separator')).toBe(false);
    expect(sheetItem('删除').style.color).toBe('var(--pad-danger-color)');
    expect(sheetItem('复制').style.color).toBe('');
    sheetItem('删除').click();
    expect(run).not.toHaveBeenCalled();
    await settle(); expect(run).toHaveBeenCalledOnce();
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
  });

  it('does not create empty menus or execute actions after unmounting', async () => {
    await mount('android');
    const run = vi.fn();
    await showMenu([[], [{label: '禁用', enabled: false, action: run}]]); await settle();
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
    await showMenu([[{label: '操作', action: run}]]); await settle();
    app!.unmount(); app = undefined; await settle();
    expect(document.querySelector('.q-bottom-sheet')).toBeNull();
    expect(run).not.toHaveBeenCalled();
  });

  it('closes outdated menus if another request or unmount overtakes native creation', async () => {
    await mount('windows');
    let finish!: (menu: object) => void;
    native.create.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const first = showMenu([[{label: '旧菜单', action: vi.fn()}]]);
    await settle();
    const menu = {popup: vi.fn(), close: vi.fn().mockResolvedValue(undefined)};
    await showMenu([[{label: '新菜单', action: vi.fn()}]]);
    finish(menu); await first;
    expect(menu.popup).not.toHaveBeenCalled();
    expect(menu.close).toHaveBeenCalledOnce();
  });

  it('cancels an already queued action when the editor unmounts', async () => {
    await mount('windows');
    const run = vi.fn();
    await showMenu([[{label: '操作', action: run}]]);
    (items()[0] as MenuItemOptions).action!('');
    app!.unmount(); app = undefined; await settle();
    expect(run).not.toHaveBeenCalled();
  });

  it('closes a native menu, invalidates queued actions, and can reopen after KeepAlive activation', async () => {
    await mount('windows');
    const run = vi.fn();
    await showMenu([[{label: '操作', action: run}]]);
    const menu = await native.create.mock.results[0].value;
    (items()[0] as MenuItemOptions).action!('');
    closeMenu(); await settle();
    expect(menu.close).toHaveBeenCalledOnce();
    expect(run).not.toHaveBeenCalled();
    await showMenu([[{label: '操作', action: run}]]);
    (items()[0] as MenuItemOptions).action!(''); await settle();
    expect(run).toHaveBeenCalledOnce();
  });

  it('cancels native creation that completes after closing', async () => {
    await mount('windows');
    let finish!: (menu: object) => void;
    native.create.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const showing = showMenu([[{label: '操作', action: vi.fn()}]]);
    await settle(); closeMenu();
    const menu = {popup: vi.fn(), close: vi.fn().mockResolvedValue(undefined)};
    finish(menu); await showing;
    expect(menu.popup).not.toHaveBeenCalled();
    expect(menu.close).toHaveBeenCalledOnce();
  });
});

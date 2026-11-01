// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {createPinia, disposePinia, setActivePinia, type Pinia} from 'pinia';
import {Dark, Quasar} from 'quasar';
import Component from '../PinnedDiarySettings.vue';
import {useConfigStore, CONFIG_LOCAL_CHANGE_EVENT} from '../../../stores/config';
import {useDataStore} from '../../../stores/data';
import {collectSyncedSettings} from '../../../utils/syncedSettings';
import type {DiarySummary} from '../../../bindings';

vi.mock('../../../utils/api', () => ({default: {cmdGetDiarySummary: vi.fn()}}));
const ids = ['8215021834823', '8215021834822', '8215021834821'];
let app: App | undefined;
let pinia: Pinia;
function summary(id: string): DiarySummary {
  return {id, title: `标题 ${id}`, created: 1, updated: 1, attachmentCount: 0, attachmentTotalSize: 0,
    attachmentCounts: {image: 0, audio: 0, video: 0, file: 0}, encryptedAttachmentCounts: {image: 0, audio: 0, video: 0, file: 0}};
}
beforeEach(() => {vi.useFakeTimers(); localStorage.clear(); pinia = createPinia(); setActivePinia(pinia);});
afterEach(() => {
  app?.unmount(); app = undefined; disposePinia(pinia); Dark.set(false); document.body.innerHTML = '';
  document.body.classList.remove('pad-list-dragging'); vi.useRealTimers(); vi.restoreAllMocks();
});
async function settle() {await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick();}
async function mount(initial = ids) {
  const config = useConfigStore(); await config.saveNormalConfig('pinned_diary_ids', [...initial]);
  useDataStore().diarySummaries = Object.fromEntries(ids.map(id => [id, summary(id)]));
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(Component); app.use(pinia).use(Quasar).mount(host);
  host.querySelector<HTMLElement>('.settings-item')!.click(); await settle();
  return config;
}
const rows = () => [...document.querySelectorAll<HTMLElement>('[data-pinned-diary-id]')];
const order = () => rows().map(row => row.dataset.pinnedDiaryId);
function button(id: string, label: string) {
  return document.querySelector<HTMLButtonElement>(`[data-pinned-diary-id="${id}"] button[aria-label^="${label}"]`)!;
}
function pointer(type: string, y: number, pointerType = 'mouse') {
  return new PointerEvent(type, {pointerId: 1, isPrimary: true, pointerType, button: 0, clientY: y, bubbles: true, cancelable: true});
}
function mockPositions() {
  vi.spyOn(document.querySelector<HTMLElement>('.pinned-list')!, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 350, 240));
  rows().forEach((row, i) => vi.spyOn(row, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, i * 64, 350, 64)));
}

describe.each([false, true])('置顶顺序设置（深色=%s）', dark => {
  it('moves with arrows, persists immediately and preserves the order in synced settings', async () => {
    Dark.set(dark); const config = await mount();
    expect(document.querySelector('.q-dialog .q-card--dark') !== null).toBe(dark);
    expect(button(ids[0]!, '上移').disabled).toBe(true); expect(button(ids[2]!, '下移').disabled).toBe(true);
    button(ids[0]!, '下移').click(); await settle();
    expect(order()).toEqual([ids[1], ids[0], ids[2]]);
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual(order());
    expect((await collectSyncedSettings(config)).pinnedDiaryIds).toEqual(order());
    button(ids[2]!, '上移').click(); await settle();
    expect(order()).toEqual([ids[1], ids[2], ids[0]]);
    expect(button(ids[0]!, '下移').disabled).toBe(true);
  });

  it.each(['mouse', 'touch'])('only saves a drag on release, not while hovering (%s)', async pointerType => {
    Dark.set(dark); const config = await mount(); mockPositions();
    const changes = vi.fn(); window.addEventListener(CONFIG_LOCAL_CHANGE_EVENT, changes);
    try {
      button(ids[0]!, '拖动').dispatchEvent(pointer('pointerdown', 32, pointerType));
      for (let i = 0; i < 5; i++) document.dispatchEvent(pointer('pointermove', 230, pointerType));
      await nextTick();
      expect(order()).toEqual(ids); expect(changes).not.toHaveBeenCalled();
      expect(document.querySelector('.drop-after')?.getAttribute('data-pinned-diary-id')).toBe(ids[2]);
      document.dispatchEvent(pointer('pointerup', 230, pointerType)); await settle();
      expect(order()).toEqual([ids[1], ids[2], ids[0]]); expect(changes).toHaveBeenCalledTimes(1);
      expect((await collectSyncedSettings(config)).pinnedDiaryIds).toEqual(order());
      expect(document.body.classList.contains('pad-list-dragging')).toBe(false);
    } finally {window.removeEventListener(CONFIG_LOCAL_CHANGE_EVENT, changes);}
  });
});

describe('置顶排序边界与取消', () => {
  it('does not reorder when the dialog closes during dragging', async () => {
    const config = await mount(); mockPositions();
    button(ids[0]!, '拖动').dispatchEvent(pointer('pointerdown', 32)); document.dispatchEvent(pointer('pointermove', 230));
    document.querySelector<HTMLElement>('[aria-label="关闭置顶日记弹窗"]')!.click(); await settle();
    document.dispatchEvent(pointer('pointerup', 230));
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual(ids);
    expect(document.body.classList.contains('pad-list-dragging')).toBe(false);
  });
  it('cancels a stale drag if another device or setting replaces the current order', async () => {
    const config = await mount(); mockPositions();
    button(ids[0]!, '拖动').dispatchEvent(pointer('pointerdown', 32)); document.dispatchEvent(pointer('pointermove', 230));
    const incoming = [ids[2]!, ids[0]!, ids[1]!];
    await config.saveNormalConfig('pinned_diary_ids', incoming); await nextTick();
    document.dispatchEvent(pointer('pointerup', 230)); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual(incoming); expect(order()).toEqual(incoming);
  });
  it('can unpin entries after reordering and disables movement for a single entry', async () => {
    const config = await mount([ids[2]!, ids[0]!, ids[1]!]);
    button(ids[0]!, '取消置顶').click(); await settle(); expect(order()).toEqual([ids[2], ids[1]]);
    button(ids[2]!, '取消置顶').click(); await settle();
    expect(button(ids[1]!, '上移').disabled).toBe(true); expect(button(ids[1]!, '下移').disabled).toBe(true); expect(button(ids[1]!, '拖动').disabled).toBe(true);
    button(ids[1]!, '取消置顶').click(); await settle();
    expect(await config.getNormalConfig('pinned_diary_ids')).toEqual([]); expect(document.querySelector('.empty-state')).not.toBeNull();
  });
});

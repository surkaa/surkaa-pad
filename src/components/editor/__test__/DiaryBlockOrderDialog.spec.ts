// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, ref, type App} from 'vue';
import {Dark, Dialog, Quasar, Screen} from 'quasar';
import Component from '../DiaryBlockOrderDialog.vue';
import componentSource from '../DiaryBlockOrderDialog.vue?raw';
import themeCss from '../../../assets/style.css?raw';
import type {DiaryBlockDescriptor} from '../blockOrder';

let app: App | undefined, styles: HTMLStyleElement;
const initiallySmall = Screen.lt.sm;
const blocks: DiaryBlockDescriptor[] = [
  {sourceIndex: 0, type: 'paragraph', title: '段落', preview: '第一段', icon: 'notes'},
  {sourceIndex: 1, type: 'imageNode', title: '图片', preview: '照片.jpg', icon: 'image'},
  {sourceIndex: 2, type: 'summaryNode', title: '折叠内容', preview: '详细记录', icon: 'unfold_more'},
];
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
function button(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find(node =>
    node.getAttribute('aria-label') === label || node.textContent?.trim() === label)!;
}
function pointer(type: string, y = 36, pointerType = 'touch') {
  return new PointerEvent(type, {pointerId: 1, button: 0, isPrimary: true, pointerType,
    clientY: y, bubbles: true, cancelable: true});
}
function order() {
  return [...document.querySelectorAll<HTMLElement>('[data-block-index]')].map(row => Number(row.dataset.blockIndex));
}
function themeColor(token: string) {
  const probe = document.createElement('span'); probe.style.color = `var(${token})`; document.body.append(probe);
  const color = getComputedStyle(probe).color; probe.remove(); return color;
}
function geometry() {
  const list = document.querySelector<HTMLElement>('.block-order-list')!;
  vi.spyOn(list, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 400, 300));
  list.querySelectorAll<HTMLElement>('[data-block-index]').forEach((row, index) => {
    vi.spyOn(row, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, index * 72, 400, 72));
  });
}
async function mount(dark: boolean, small: boolean) {
  Screen.lt.sm = small;
  const visible = ref(true), confirm = vi.fn();
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({setup: () => () => h(Component, {
    blocks, modelValue: visible.value, onConfirm: confirm,
    'onUpdate:modelValue': (value: boolean) => { visible.value = value; },
  })});
  app.use(Quasar, {plugins: {Dialog}}); Dark.set(dark); app.mount(host); await settle(); geometry();
  return {visible, confirm};
}

beforeEach(() => {
  vi.useFakeTimers();
  styles = document.createElement('style');
  styles.textContent = themeCss + '\n' + componentSource.match(/<style scoped lang="scss">([\s\S]*?)<\/style>/)![1]
    .replace(/:global\(([^)]+)\)/g, '$1');
  document.head.append(styles);
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); Screen.lt.sm = initiallySmall;
  document.body.innerHTML = ''; styles.remove(); vi.restoreAllMocks(); vi.useRealTimers();
});

describe.each([[false, false], [true, false], [false, true], [true, true]])(
  'diary block order dialog (dark=%s, narrow=%s)', (dark, small) => {
    it('reorders a local draft by handle without mutating the diary before confirmation', async () => {
      const {confirm} = await mount(dark, small);
      const original = structuredClone(blocks);
      expect(button('确认调整').disabled).toBe(true);
      expect(document.querySelector('.q-dialog__inner--maximized') !== null).toBe(small);
      expect(getComputedStyle(button('拖动第 1 个内容块')).touchAction).toBe('none');
      expect(getComputedStyle(document.querySelector('.block-order-list')!).overflowY).toBe('auto');
      expect(getComputedStyle(document.querySelector('.block-order-list')!).backgroundColor).toBe(themeColor('--pad-bg-color-100'));
      expect(getComputedStyle(document.querySelector('.block-order-title')!).color).toBe(themeColor('--pad-text-color-100'));
      button('拖动第 1 个内容块').dispatchEvent(pointer('pointerdown'));
      for (let count = 0; count < 3; count++) {
        document.dispatchEvent(pointer('pointermove', 240)); await nextTick();
        expect(order()).toEqual([0, 1, 2]);
        expect(document.querySelector('[data-block-index="0"]')?.classList.contains('is-dragging')).toBe(true);
        expect(document.querySelector('[data-block-index="2"]')?.classList.contains('drop-after')).toBe(true);
      }
      expect(button('确认调整').disabled).toBe(true);
      expect(button('下移第 1 个内容块').disabled).toBe(true);
      expect(confirm).not.toHaveBeenCalled();
      document.dispatchEvent(pointer('pointerup', 240)); await settle();
      expect(order()).toEqual([1, 2, 0]);
      expect(button('确认调整').disabled).toBe(false);
      expect(blocks).toEqual(original); expect(confirm).not.toHaveBeenCalled();
      button('确认调整').click(); await nextTick();
      expect(confirm).toHaveBeenCalledExactlyOnceWith([1, 2, 0]);
      expect(document.body.classList.contains('body--dark')).toBe(dark);
    });

    it('keeps arrow controls and restores the original order after a drag', async () => {
      const {confirm} = await mount(dark, small);
      button('拖动第 3 个内容块').dispatchEvent(pointer('pointerdown', 180, 'mouse'));
      document.dispatchEvent(pointer('pointermove', 5, 'mouse'));
      document.dispatchEvent(pointer('pointerup', 5, 'mouse')); await settle();
      expect(order()).toEqual([2, 0, 1]);
      button('下移第 1 个内容块').click(); await settle();
      expect(order()).toEqual([0, 2, 1]);
      button('恢复原顺序').click(); await settle();
      expect(order()).toEqual([0, 1, 2]); expect(button('确认调整').disabled).toBe(true);
      expect(confirm).not.toHaveBeenCalled();
    });

    it('cancels without confirmation and resets the draft when reopened', async () => {
      const {visible, confirm} = await mount(dark, small);
      button('拖动第 1 个内容块').dispatchEvent(pointer('pointerdown'));
      document.dispatchEvent(pointer('pointermove', 240));
      document.dispatchEvent(pointer('pointerup', 240)); await settle();
      button('取消').click(); await settle();
      expect(visible.value).toBe(false); expect(confirm).not.toHaveBeenCalled();
      visible.value = true; await settle(); geometry();
      expect(order()).toEqual([0, 1, 2]); expect(button('确认调整').disabled).toBe(true);
      button('拖动第 1 个内容块').dispatchEvent(pointer('pointerdown'));
      document.dispatchEvent(pointer('pointermove', 240));
      visible.value = false; await settle();
      document.dispatchEvent(pointer('pointerup', 240)); await settle();
      expect(confirm).not.toHaveBeenCalled();
      expect(document.body.classList.contains('pad-list-dragging')).toBe(false);
    });
  },
);

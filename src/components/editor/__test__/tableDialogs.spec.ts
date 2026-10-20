// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App, type Component} from 'vue';
import {Dark, Quasar} from 'quasar';
import TableInsertDialog from '../TableInsertDialog.vue';
import dialogSource from '../TableInsertDialog.vue?raw';

let app: App | undefined;
let dialogStyle: HTMLStyleElement;
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(component: Component, props: object, dark: boolean) {
  const visible = ref(true);
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(defineComponent({setup: () => () => h(component, {
    ...props, modelValue: visible.value, 'onUpdate:modelValue': (value: boolean) => { visible.value = value; },
  })}));
  app.use(Quasar); Dark.set(dark); app.mount(host); await settle();
  return visible;
}
function button(label: string) {
  return Array.from(document.querySelectorAll<HTMLButtonElement>('button'))
    .find(element => element.getAttribute('aria-label') === label || element.textContent?.trim() === label)!;
}
beforeEach(() => {
  vi.useFakeTimers();
  // Vitest 默认不注入 SFC 样式；使用组件自身的 CSS 检查预览容器的固定尺寸。
  dialogStyle = document.createElement('style');
  dialogStyle.textContent = dialogSource.match(/<style scoped>([\s\S]*?)<\/style>/)![1];
  document.head.append(dialogStyle);
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false);
  dialogStyle.remove();
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe.each([false, true])('table dialogs (dark=%s)', dark => {
  it('starts at 3×3, supports steppers and resets on reopening', async () => {
    const insert = vi.fn();
    const visible = await mount(TableInsertDialog, {onInsert: insert}, dark);
    const inputs = Array.from(document.querySelectorAll<HTMLInputElement>('input'));
    expect(inputs.map(input => input.value)).toEqual(['3', '3']);
    const preview = document.querySelector('.table-preview')!;
    expect(preview.querySelectorAll('tr')).toHaveLength(3);
    expect(preview.querySelectorAll('th')).toHaveLength(3);
    expect(preview.querySelectorAll('td')).toHaveLength(6);
    for (const input of inputs) expect(input.closest('.q-field')?.classList.contains('q-field--dark')).toBe(dark);
    button('增加行数').click(); button('减少列数').click(); await settle();
    expect(inputs.map(input => input.value)).toEqual(['4', '2']);
    expect(document.querySelector('.table-preview')).toBe(preview);
    expect(preview.querySelectorAll('tr')).toHaveLength(4);
    expect(preview.querySelectorAll('th')).toHaveLength(2);
    expect(preview.querySelectorAll('td')).toHaveLength(6);
    expect(document.querySelector('.table-preview-size')?.textContent?.trim()).toBe('4 行 × 2 列');
    button('插入').click(); await settle();
    expect(insert).toHaveBeenCalledExactlyOnceWith(4, 2);
    visible.value = false; await settle(); visible.value = true; await settle();
    expect(Array.from(document.querySelectorAll<HTMLInputElement>('input'), input => input.value)).toEqual(['3', '3']);
    expect(document.querySelectorAll('.table-preview tr')).toHaveLength(3);
  });

  it('rejects empty, fractional and oversized dimensions and cancels without insertion', async () => {
    const insert = vi.fn();
    await mount(TableInsertDialog, {onInsert: insert}, dark);
    const input = document.querySelector<HTMLInputElement>('input')!;
    for (const value of ['', '0', '1.5', '101']) {
      input.value = value; input.dispatchEvent(new Event('input', {bubbles: true})); await settle();
      expect(button('插入').disabled).toBe(true);
      expect(document.querySelector('.table-preview-grid')).toBeNull();
      expect(document.querySelector('.table-preview-message')?.textContent).toContain('请输入有效');
      button('插入').click(); expect(insert).not.toHaveBeenCalled();
    }
    button('取消').click(); await settle(); expect(insert).not.toHaveBeenCalled();
  });

  it('keeps a bounded read-only viewport for the largest valid table and a header-only table', async () => {
    const insert = vi.fn();
    await mount(TableInsertDialog, {onInsert: insert}, dark);
    const inputs = document.querySelectorAll<HTMLInputElement>('input');
    const preview = document.querySelector<HTMLElement>('.table-preview')!;
    const setSize = async (rows: string, columns: string) => {
      for (const [input, value] of [[inputs[0], rows], [inputs[1], columns]] as const) {
        input.value = value; input.dispatchEvent(new Event('input', {bubbles: true}));
      }
      await settle();
    };
    await setSize('100', '10');
    expect(document.querySelector('.table-preview')).toBe(preview);
    expect(preview.querySelectorAll('tr')).toHaveLength(100);
    expect(preview.querySelectorAll('th, td')).toHaveLength(1000);
    expect(preview.querySelector<HTMLTableElement>('table')?.style.width).toBe('640px');
    expect(getComputedStyle(preview).height).toBe('184px');
    expect(getComputedStyle(preview).overflow).toBe('auto');
    expect(preview.querySelector('[contenteditable], input, textarea')).toBeNull();
    expect(insert).not.toHaveBeenCalled();
    await setSize('100', '20'); // 超过 1000 格时不继续渲染，防止预览绕过插入限制。
    expect(preview.querySelector('table')).toBeNull();
    await setSize('1', '1');
    expect(preview.querySelectorAll('th')).toHaveLength(1);
    expect(preview.querySelectorAll('td')).toHaveLength(0);
    expect(getComputedStyle(preview).height).toBe('184px');
    button('插入').click(); await settle();
    expect(insert).toHaveBeenCalledExactlyOnceWith(1, 1);
  });
});

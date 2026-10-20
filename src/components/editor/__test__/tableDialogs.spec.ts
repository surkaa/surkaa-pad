// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App, type Component} from 'vue';
import {Dark, Quasar} from 'quasar';
import TableInsertDialog from '../TableInsertDialog.vue';

let app: App | undefined;
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
beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false);
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe.each([false, true])('table dialogs (dark=%s)', dark => {
  it('starts at 3×3, supports steppers and resets on reopening', async () => {
    const insert = vi.fn();
    const visible = await mount(TableInsertDialog, {onInsert: insert}, dark);
    const inputs = Array.from(document.querySelectorAll<HTMLInputElement>('input'));
    expect(inputs.map(input => input.value)).toEqual(['3', '3']);
    for (const input of inputs) expect(input.closest('.q-field')?.classList.contains('q-field--dark')).toBe(dark);
    button('增加行数').click(); button('减少列数').click(); await settle();
    expect(inputs.map(input => input.value)).toEqual(['4', '2']);
    button('插入').click(); await settle();
    expect(insert).toHaveBeenCalledExactlyOnceWith(4, 2);
    visible.value = false; await settle(); visible.value = true; await settle();
    expect(Array.from(document.querySelectorAll<HTMLInputElement>('input'), input => input.value)).toEqual(['3', '3']);
  });

  it('rejects empty, fractional and oversized dimensions and cancels without insertion', async () => {
    const insert = vi.fn();
    await mount(TableInsertDialog, {onInsert: insert}, dark);
    const input = document.querySelector<HTMLInputElement>('input')!;
    for (const value of ['', '0', '1.5', '101']) {
      input.value = value; input.dispatchEvent(new Event('input', {bubbles: true})); await settle();
      expect(button('插入').disabled).toBe(true);
      button('插入').click(); expect(insert).not.toHaveBeenCalled();
    }
    button('取消').click(); await settle(); expect(insert).not.toHaveBeenCalled();
  });

});

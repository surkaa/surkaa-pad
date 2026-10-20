// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import Component from '../DetailsHelpButton.vue';
import componentSource from '../DetailsHelpButton.vue?raw';
import themeCss from '../../assets/style.css?raw';

let app: App | undefined;
let styles: HTMLStyleElement;
const title = '详细说明';
async function settle() { await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(inDialog: boolean, dark = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({render: () => h(Component, {title, inDialog}, () => h('p', '可以滚动阅读和复制的详细介绍'))});
  app.use(Quasar); app.mount(host); Dark.set(dark); await settle();
}
function trigger() { return document.querySelector<HTMLButtonElement>(`button[aria-label="${title}"]`)!; }
async function outsideClick(target: HTMLElement) {
  target.dispatchEvent(new PointerEvent('pointerdown', {bubbles: true}));
  target.click(); await settle();
}
beforeEach(() => {
  vi.useFakeTimers();
  // Vitest 不注入 SFC 样式；在真实主题变量下检查 Teleport 浮层的颜色与交互样式。
  styles = document.createElement('style');
  styles.textContent = themeCss + '\n' + componentSource.match(/<style scoped>([\s\S]*?)<\/style>/)![1]
    .replace(/:(?:global|deep)\(([^)]+)\)/g, '$1');
  document.head.append(styles);
});
afterEach(() => {
  app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; styles.remove(); vi.useRealTimers();
});

function themeColor(token: string) {
  const probe = document.createElement('span'); probe.style.color = `var(${token})`; document.body.append(probe);
  const value = getComputedStyle(probe).color; probe.remove(); return value;
}

describe('details help button', () => {
  it.each([false, true])('toggles a persistent, readable Tooltip by click inside a dialog, dark=%s', async dark => {
    await mount(true, dark);
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
    trigger().click(); await settle();
    const tooltip = document.querySelector<HTMLElement>('[role="tooltip"]')!;
    expect(tooltip.textContent).toContain('可以滚动阅读和复制');
    expect(tooltip.classList.contains('details-help-tooltip')).toBe(true);
    const appearance = getComputedStyle(tooltip);
    expect(appearance.backgroundColor).toBe(themeColor('--pad-bg-color-200'));
    expect(appearance.color).toBe(themeColor('--pad-text-color-200'));
    expect(appearance.pointerEvents).toBe('auto');
    expect(appearance.overflowY).toBe('auto');
    expect(appearance.userSelect).toBe('text');
    expect(trigger().getAttribute('aria-describedby')).toBe(tooltip.id);
    expect(trigger().getAttribute('aria-expanded')).toBe('true');
    expect(document.querySelector('.q-dialog')).toBeNull();
    tooltip.dispatchEvent(new MouseEvent('mouseleave')); await settle();
    expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
    await outsideClick(tooltip.querySelector('p')!);
    expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
    await outsideClick(tooltip);
    expect(document.querySelector('[role="tooltip"]')).not.toBeNull();
    trigger().click(); await settle();
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
    expect(trigger().getAttribute('aria-expanded')).toBe('false');
    expect(document.body.classList.contains('body--dark')).toBe(dark);
  });

  it('closes on outside click and Escape without passing Escape to a parent dialog', async () => {
    await mount(true);
    trigger().click(); await settle();
    await outsideClick(document.body);
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
    trigger().click(); await settle();
    const parentKeydown = vi.fn(); window.addEventListener('keydown', parentKeydown);
    try {
      trigger().dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', keyCode: 27, bubbles: true, cancelable: true}));
      await settle();
      expect(document.querySelector('[role="tooltip"]')).toBeNull();
      expect(parentKeydown).not.toHaveBeenCalled();
      expect(document.activeElement).toBe(trigger());
    } finally { window.removeEventListener('keydown', parentKeydown); }
  });

  it.each([false, true])('uses a secondary dialog outside a dialog, dark=%s', async dark => {
    await mount(false, dark);
    expect(trigger().getAttribute('aria-haspopup')).toBe('dialog');
    trigger().click(); await settle();
    expect(document.querySelector('.details-help-dialog')?.textContent).toContain('可以滚动阅读和复制');
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
    expect(document.body.classList.contains('body--dark')).toBe(dark);
    document.querySelector<HTMLButtonElement>(`button[aria-label="关闭${title}"]`)!.click(); await settle();
    expect(document.querySelector('.details-help-dialog')).toBeNull();
    expect(trigger().getAttribute('aria-expanded')).toBe('false');
  });

  it('removes the Tooltip portal and listeners when its parent is unmounted', async () => {
    await mount(true); trigger().click(); await settle();
    app!.unmount(); app = undefined; await settle();
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
    const event = new KeyboardEvent('keydown', {key: 'Escape', cancelable: true});
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });
});

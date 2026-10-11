// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Quasar, Dark} from 'quasar';
import Component from '../VaultKeyChangeDialog.vue';

const api = vi.hoisted(() => ({
  cmdGetVaultKeyChangeStatus: vi.fn(async () => ({pending: false, remote: false})),
  cmdDiscardVaultKeyChangePlan: vi.fn(async () => {}),
}));
vi.mock('../../utils/api', () => ({default: api}));
vi.mock('../../stores/timeout', () => ({useTimeoutStore: () => ({clearCloseTimers: vi.fn(), setTimeoutForCloseApp: vi.fn()})}));

let app: App | undefined;
async function mount(dark = false) {
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(Component, {modelValue: true});
  app.use(Quasar, {config: {dark}});
  app.mount(host);
  app.config.globalProperties.$q.dark.set(dark);
  await vi.advanceTimersByTimeAsync(500);
  await nextTick();
}

describe('vault key change dialog', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    api.cmdGetVaultKeyChangeStatus.mockReset().mockResolvedValue({pending: false, remote: false});
    api.cmdDiscardVaultKeyChangePlan.mockReset().mockResolvedValue(undefined);
  });
  afterEach(() => {
    app?.unmount(); app = undefined;
    Dark.set(false);
    document.body.innerHTML = '';
    vi.useRealTimers();
  });

  it('starts disabled until password confirmation and other-device acknowledgement are supplied', async () => {
    await mount();
    const inputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
    const submit = [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.includes('统计数据并估算'))!;
    expect(submit.disabled).toBe(true);
    const fill = (i: number, value: string) => { inputs[i].value = value; inputs[i].dispatchEvent(new Event('input', {bubbles: true})); };
    fill(0, 'old-test'); fill(1, 'new-test'); fill(2, 'different-test');
    document.querySelector<HTMLElement>('.q-checkbox')!.click();
    await nextTick();
    expect(submit.disabled).toBe(true);
    fill(2, 'new-test'); await nextTick();
    expect(submit.disabled).toBe(false);
  });

  it('uses a separately themed Teleport menu in dark mode', async () => {
    await mount(true);
    document.querySelector<HTMLElement>('.q-select')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const popup = document.querySelector('.key-change-memory-popup');
    expect(popup).not.toBeNull();
    expect(popup!.classList.contains('q-menu--dark')).toBe(true);
    expect(document.querySelectorAll('.q-field--dark').length).toBe(4);
  });

  it('shows new-password recovery rather than the normal old-password form when a journal is pending', async () => {
    api.cmdGetVaultKeyChangeStatus.mockResolvedValue({pending: true, remote: true});
    await mount();
    expect(document.body.textContent).toContain('改密后的主密码');
    expect(document.body.textContent).not.toContain('当前主密码');
    expect(document.querySelectorAll('input[type="password"]').length).toBe(1);
    expect([...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '继续改密')?.disabled).toBe(true);
  });

  it('keeps and displays a plan-discard error instead of leaking an unhandled rejection', async () => {
    api.cmdDiscardVaultKeyChangePlan.mockRejectedValue(new Error('连接中断'));
    await mount();
    [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === '取消')!.click();
    await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('连接中断');
  });
});

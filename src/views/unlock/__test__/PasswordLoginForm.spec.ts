// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Quasar, Dark} from 'quasar';
import Component from '../PasswordLoginForm.vue';

let app: App | undefined;
const dialog = vi.fn();
async function mount(showChangedKeyHint: boolean, dark = false, loading = false) {
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(Component, {masterPassword: '', loading, biometricEnabled: false,
    biometricUnlockAllowed: false, biometricPasswordIntervalDays: 7, showChangedKeyHint});
  app.use(Quasar);
  app.config.globalProperties.$q.dialog = dialog;
  app.mount(host);
  Dark.set(dark);
  await nextTick();
}
afterEach(() => {
  app?.unmount(); app = undefined;
  Dark.set(false); document.body.innerHTML = ''; dialog.mockClear();
});
describe('other-device key change help', () => {
  it.each([false, true])('only opens the explanation on click (dark=%s)', async dark => {
    await mount(true, dark);
    expect(document.body.textContent).not.toContain('若其他设备已改密');
    const button = document.querySelector<HTMLButtonElement>('[aria-label="其他设备改密后的解锁说明"]')!;
    expect(button.previousElementSibling?.textContent).toContain('重置配置');
    button.click();
    expect(dialog).toHaveBeenCalledWith(expect.objectContaining({message: expect.stringContaining('先输入本机原密码')}));
  });
  it('hides the help when the local vault does not need it', async () => {
    await mount(false);
    expect(document.querySelector('[aria-label="其他设备改密后的解锁说明"]')).toBeNull();
  });
  it('disables help while unlocking', async () => {
    await mount(true, false, true);
    expect(document.querySelector<HTMLButtonElement>('[aria-label="其他设备改密后的解锁说明"]')!.disabled).toBe(true);
  });
});

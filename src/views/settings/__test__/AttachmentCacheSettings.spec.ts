// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Quasar, Dark} from 'quasar';
import Component from '../AttachmentCacheSettings.vue';

vi.mock('../../../utils/api', () => ({default: {
  cmdGetAttachmentCacheInfo: vi.fn(async () => ({cachedFiles: 1, cachedBytes: 1024, limitBytes: 1073741824, maxFileSizeBytes: 104857600})),
}}));
let app: App | undefined;
afterEach(() => {
  app?.unmount(); app = undefined;
  Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers();
});
describe('attachment cache select popup themes', () => {
  it.each([false, true])('themes both popup menus explicitly (dark=%s)', async dark => {
    vi.useFakeTimers();
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(Component); app.use(Quasar); app.mount(host);
    Dark.set(dark);
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    const fields = document.querySelectorAll<HTMLElement>('.q-select');
    for (const field of fields) {
      expect(field.classList.contains('q-field--dark')).toBe(dark);
      field.click(); await vi.advanceTimersByTimeAsync(500); await nextTick();
      const menu = document.querySelector('.settings-select-popup')!;
      expect(menu).not.toBeNull();
      expect(menu.classList.contains('q-menu--dark')).toBe(dark);
      field.click(); await vi.advanceTimersByTimeAsync(500); await nextTick();
    }
  });
});

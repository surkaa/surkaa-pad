// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {effectScope, ref, type EffectScope} from 'vue';
import {useAndroidBackButton} from '../useAndroidBackButton';

const mocks = vi.hoisted(() => ({isTauri: vi.fn(), platform: vi.fn(), listen: vi.fn(), unregister: vi.fn()}));
vi.mock('@tauri-apps/api/core', () => ({isTauri: mocks.isTauri}));
vi.mock('@tauri-apps/plugin-os', () => ({platform: mocks.platform}));
vi.mock('@tauri-apps/api/app', () => ({onBackButtonPress: mocks.listen}));
let scope: EffectScope | undefined;
beforeEach(() => {
  vi.resetAllMocks(); mocks.isTauri.mockReturnValue(true); mocks.platform.mockReturnValue('android');
  mocks.unregister.mockResolvedValue(undefined); mocks.listen.mockResolvedValue({unregister: mocks.unregister});
});
afterEach(() => {scope?.stop(); scope = undefined; vi.restoreAllMocks();});
const settle = async () => {await Promise.resolve(); await Promise.resolve();};

describe('scoped Android back listener', () => {
  it.each(['windows', 'linux', 'macos', 'browser'])('does not replace native navigation on %s', async platform => {
    mocks.platform.mockReturnValue(platform); mocks.isTauri.mockReturnValue(platform !== 'browser');
    scope = effectScope(); scope.run(() => useAndroidBackButton(ref(true), vi.fn())); await settle();
    expect(mocks.listen).not.toHaveBeenCalled();
  });

  it('registers only while visible and ignores callbacks after closing', async () => {
    const visible = ref(false); const back = vi.fn(); scope = effectScope();
    scope.run(() => useAndroidBackButton(visible, back)); expect(mocks.listen).not.toHaveBeenCalled();
    visible.value = true; await settle();
    const callback = mocks.listen.mock.calls[0]![0]; callback({canGoBack: true}); expect(back).toHaveBeenCalledTimes(1);
    visible.value = false; await settle();
    expect(mocks.unregister).toHaveBeenCalledTimes(1);
    callback({canGoBack: true}); expect(back).toHaveBeenCalledTimes(1);
    scope.stop(); scope = undefined; expect(mocks.unregister).toHaveBeenCalledTimes(1);
  });

  it.each(['close', 'unmount'])('unregisters a late registration after %s', async reason => {
    let finish!: (listener: unknown) => void;
    mocks.listen.mockImplementation(() => new Promise(resolve => {finish = resolve;}));
    const visible = ref(true); const back = vi.fn(); scope = effectScope();
    scope.run(() => useAndroidBackButton(visible, back));
    if (reason === 'close') visible.value = false; else {scope.stop(); scope = undefined;}
    mocks.listen.mock.calls[0]![0]({canGoBack: false}); expect(back).not.toHaveBeenCalled();
    finish({unregister: mocks.unregister}); await settle(); expect(mocks.unregister).toHaveBeenCalledTimes(1);
  });

  it('does not reactivate an old listener when the preview closes and reopens quickly', async () => {
    const pending: Array<(listener: unknown) => void> = [];
    mocks.listen.mockImplementation(() => new Promise(resolve => pending.push(resolve)));
    const visible = ref(true); const back = vi.fn(); scope = effectScope();
    scope.run(() => useAndroidBackButton(visible, back)); visible.value = false; visible.value = true;
    const oldUnregister = vi.fn().mockResolvedValue(undefined); const newUnregister = vi.fn().mockResolvedValue(undefined);
    pending[1]!({unregister: newUnregister}); pending[0]!({unregister: oldUnregister}); await settle();
    expect(oldUnregister).toHaveBeenCalledTimes(1); expect(newUnregister).not.toHaveBeenCalled();
    mocks.listen.mock.calls[0]![0]({canGoBack: true}); mocks.listen.mock.calls[1]![0]({canGoBack: true});
    expect(back).toHaveBeenCalledTimes(1);
    scope.stop(); scope = undefined; expect(newUnregister).toHaveBeenCalledTimes(1);
  });

  it('reports registration failures without invoking back or causing an unhandled rejection', async () => {
    const error = new Error('IPC unavailable'); const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mocks.listen.mockRejectedValue(error); const back = vi.fn(); scope = effectScope();
    scope.run(() => useAndroidBackButton(ref(true), back)); await settle();
    expect(log).toHaveBeenCalledWith('监听附件预览系统返回失败：', error); expect(back).not.toHaveBeenCalled();
  });
});

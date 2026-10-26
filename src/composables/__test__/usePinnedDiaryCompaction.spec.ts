// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {effectScope, nextTick, ref, type EffectScope} from 'vue';
import {shouldCompactPinnedDiaries, usePinnedDiaryCompaction} from '../usePinnedDiaryCompaction';

let scope: EffectScope;
let callbacks: Array<() => void>;
const observe = vi.fn();
const disconnect = vi.fn();
const resize = () => callbacks[callbacks.length - 1]();

beforeEach(() => {
  vi.clearAllMocks(); callbacks = [];
  vi.stubGlobal('ResizeObserver', class {
    observe = observe;
    disconnect = disconnect;
    constructor(callback: ResizeObserverCallback) { callbacks.push(() => callback([], this as unknown as ResizeObserver)); }
  });
});
afterEach(() => { scope?.stop(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

function setup(naturalHeight = 300, viewportHeight = 600) {
  const viewport = ref<HTMLElement | null>(document.createElement('div'));
  const natural = ref<HTMLElement | null>(document.createElement('div'));
  const sizes = {naturalHeight, viewportHeight};
  Object.defineProperty(viewport.value, 'clientHeight', {get: () => sizes.viewportHeight});
  // scrollHeight 故意很大，判断应只使用列表可视高度，而不是内容高度或整个窗口高度。
  Object.defineProperty(viewport.value, 'scrollHeight', {get: () => 10_000});
  vi.spyOn(natural.value!, 'getBoundingClientRect').mockImplementation(() => new DOMRect(0, 0, 320, sizes.naturalHeight));
  scope = effectScope();
  const compact = scope.run(() => usePinnedDiaryCompaction(viewport, natural))!;
  return {viewport, natural, sizes, compact};
}

describe('置顶日记统一紧缩阈值', () => {
  it.each([[299, 600, false], [300, 600, false], [301, 600, true], [0, 600, false],
    [100, 0, false], [100, -1, false], [NaN, 600, false], [100, Infinity, false]])(
    '正常高度 %s、可视高度 %s 时紧缩=%s', (natural, viewport, expected) => {
      expect(shouldCompactPinnedDiaries(natural as number, viewport as number)).toBe(expected);
    },
  );

  it('观察正常布局和列表可视区域，不使用 scrollHeight', () => {
    const {viewport, natural, compact} = setup(301, 600);
    expect(compact.value).toBe(true);
    expect(observe.mock.calls.map(([element]) => element)).toEqual([viewport.value, natural.value]);
  });

  it('紧缩后继续用正常布局判断，不因可见卡片变小而来回切换', () => {
    const {compact, sizes} = setup(500, 600);
    for (let frame = 0; frame < 20; frame++) {
      resize();
      expect(compact.value).toBe(true);
    }
    sizes.naturalHeight = 300;
    resize();
    expect(compact.value).toBe(false);
  });

  it('增减置顶、摘要加载及窗口或键盘高度变化后重新判断', () => {
    const {sizes, compact} = setup(290, 600);
    expect(compact.value).toBe(false);
    sizes.naturalHeight = 350; resize();
    expect(compact.value).toBe(true);
    sizes.viewportHeight = 800; resize();
    expect(compact.value).toBe(false);
    sizes.viewportHeight = 500; resize();
    expect(compact.value).toBe(true);
  });

  it('置顶为空时复原，并在退出后清理观察器', async () => {
    const {natural, compact} = setup(400, 600);
    expect(compact.value).toBe(true);
    natural.value = null;
    await nextTick();
    expect(compact.value).toBe(false);
    expect(disconnect).toHaveBeenCalled();
    scope.stop();
    const {compact: again} = setup(100, 600);
    expect(again.value).toBe(false);
    scope.stop();
    expect(disconnect).toHaveBeenCalledTimes(2);
  });
});

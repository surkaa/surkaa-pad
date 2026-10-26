import {onScopeDispose, readonly, ref, watch, type Ref} from 'vue';

export function shouldCompactPinnedDiaries(naturalHeight: number, viewportHeight: number): boolean {
  return Number.isFinite(naturalHeight) && Number.isFinite(viewportHeight)
    && viewportHeight > 0 && naturalHeight > viewportHeight * 0.5;
}

/** 用独立的正常卡片高度判断，不能用紧缩后的高度反馈，否则会反复展开/收起。 */
export function usePinnedDiaryCompaction(
  viewport: Ref<HTMLElement | null>,
  naturalLayout: Ref<HTMLElement | null>,
) {
  const compact = ref(false);
  let observer: ResizeObserver | null = null;

  function measure() {
    const layout = naturalLayout.value;
    if (!layout) { compact.value = false; return; }
    const viewportHeight = viewport.value?.clientHeight ?? 0;
    const naturalHeight = layout.getBoundingClientRect().height;
    // KeepAlive 把列表移入离屏容器时，ResizeObserver 会得到零尺寸。
    // 此时必须保留上一次布局，否则返回后再次紧缩会改变内容高度并触发滚动锚定。
    if (viewportHeight <= 0 || naturalHeight <= 0) return;
    compact.value = shouldCompactPinnedDiaries(naturalHeight, viewportHeight);
  }

  watch([viewport, naturalLayout], ([container, layout]) => {
    observer?.disconnect();
    observer = null;
    if (container && layout) {
      observer = new ResizeObserver(measure);
      observer.observe(container);
      observer.observe(layout);
    }
    measure();
  }, {immediate: true, flush: 'post'});

  onScopeDispose(() => observer?.disconnect());
  return readonly(compact);
}

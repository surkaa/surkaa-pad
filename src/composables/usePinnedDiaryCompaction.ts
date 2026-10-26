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
    compact.value = shouldCompactPinnedDiaries(
      naturalLayout.value?.getBoundingClientRect().height ?? 0,
      viewport.value?.clientHeight ?? 0,
    );
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

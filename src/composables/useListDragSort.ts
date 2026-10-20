import {computed, onBeforeUnmount, ref, shallowRef, watch} from 'vue';

/** 先预览插入点，松手后再换位，避免行位置改变反复影响拖拽命中。 */
export function useListDragSort<Key extends string | number>(options: {
  keys: () => readonly Key[];
  list: () => HTMLElement | null;
  visible: () => boolean;
  itemAttribute: string;
  onDrop: (key: Key, insertionIndex: number) => void;
}) {
  const draggedKey = shallowRef<Key | null>(null);
  const insertionIndex = ref<number | null>(null);
  let pointerId: number | null = null;
  let captureTarget: HTMLElement | null = null;
  const dropIndicator = computed(() => {
    if (draggedKey.value === null || insertionIndex.value === null) return null;
    const remaining = options.keys().filter(key => key !== draggedKey.value);
    const next = remaining[insertionIndex.value];
    if (next !== undefined) return {key: next, position: 'before' as const};
    const last = remaining[remaining.length - 1];
    return last === undefined ? null : {key: last, position: 'after' as const};
  });

  function startDrag(event: PointerEvent, key: Key) {
    if (pointerId !== null || event.button !== 0 || event.isPrimary === false
      || !options.visible() || !options.keys().includes(key) || options.keys().length < 2) return;
    event.preventDefault();
    pointerId = event.pointerId;
    draggedKey.value = key;
    insertionIndex.value = options.keys().indexOf(key);
    captureTarget = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    captureTarget?.setPointerCapture?.(pointerId);
    captureTarget?.addEventListener('lostpointercapture', cancelDrag);
    document.body.classList.add('pad-list-dragging');
    document.addEventListener('pointermove', handleMove, {passive: false});
    document.addEventListener('pointerup', handleEnd);
    document.addEventListener('pointercancel', handleEnd);
    window.addEventListener('blur', cancelDrag);
    window.addEventListener('keydown', handleKeydown, true);
  }

  function handleMove(event: PointerEvent) {
    if (event.pointerId !== pointerId || draggedKey.value === null) return;
    event.preventDefault();
    const list = options.list();
    if (!list) return;
    const bounds = list.getBoundingClientRect();
    if (event.clientY < bounds.top + 48) list.scrollTop -= 12;
    else if (event.clientY > bounds.bottom - 48) list.scrollTop += 12;

    const remaining = options.keys().filter(key => key !== draggedKey.value);
    const rows = list.querySelectorAll<HTMLElement>(`[${options.itemAttribute}]`);
    let index = remaining.length;
    for (const row of rows) {
      const rowKey = row.getAttribute(options.itemAttribute);
      const rowIndex = remaining.findIndex(key => String(key) === rowKey);
      if (rowIndex < 0) continue;
      const bounds = row.getBoundingClientRect();
      if (event.clientY < bounds.top + bounds.height / 2) {
        index = rowIndex;
        break;
      }
    }
    insertionIndex.value = index;
  }

  function handleEnd(event: PointerEvent) {
    if (event.pointerId !== pointerId) return;
    completeDrag(event.type === 'pointerup' && options.visible());
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopImmediatePropagation();
    cancelDrag();
  }

  function cancelDrag() { completeDrag(false); }

  function completeDrag(commit: boolean) {
    if (pointerId === null) return;
    const key = draggedKey.value;
    const index = insertionIndex.value;
    const id = pointerId;
    pointerId = null;
    draggedKey.value = null;
    insertionIndex.value = null;
    captureTarget?.removeEventListener('lostpointercapture', cancelDrag);
    if (id !== null && captureTarget?.hasPointerCapture?.(id)) captureTarget.releasePointerCapture(id);
    captureTarget = null;
    document.body.classList.remove('pad-list-dragging');
    document.removeEventListener('pointermove', handleMove);
    document.removeEventListener('pointerup', handleEnd);
    document.removeEventListener('pointercancel', handleEnd);
    window.removeEventListener('blur', cancelDrag);
    window.removeEventListener('keydown', handleKeydown, true);
    if (commit && key !== null && index !== null && options.keys().includes(key)
      && options.keys().indexOf(key) !== index) options.onDrop(key, index);
  }

  watch(options.visible, visible => { if (!visible) cancelDrag(); });
  watch(() => [...options.keys()], () => cancelDrag());
  onBeforeUnmount(cancelDrag);
  return {draggedKey, dropIndicator, startDrag, cancelDrag};
}

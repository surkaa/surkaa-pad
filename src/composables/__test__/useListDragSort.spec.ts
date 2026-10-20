// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, ref, type App} from 'vue';
import {useListDragSort} from '../useListDragSort';

let app: App | undefined;
afterEach(() => {
  app?.unmount(); app = undefined; document.body.innerHTML = '';
  document.body.classList.remove('pad-list-dragging'); vi.restoreAllMocks();
});

function pointer(type: string, y: number, options: PointerEventInit = {}) {
  return new PointerEvent(type, {pointerId: 1, isPrimary: true, button: 0, pointerType: 'mouse',
    clientY: y, bubbles: true, cancelable: true, ...options});
}

function mount(initial = [0, 1, 2]) {
  const keys = ref(initial), visible = ref(true), list = ref<HTMLElement | null>(null);
  const onDrop = vi.fn((key: number, index: number) => {
    const next = keys.value.filter(item => item !== key); next.splice(index, 0, key); keys.value = next;
  });
  let sort!: ReturnType<typeof useListDragSort<number>>;
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({setup() {
    sort = useListDragSort({keys: () => keys.value, visible: () => visible.value,
      list: () => list.value, itemAttribute: 'data-key', onDrop});
    return () => h('div', {ref: list}, keys.value.map(key => h('div', {'data-key': key}, [
      h('button', {onPointerdown: (event: PointerEvent) => sort.startDrag(event, key)}, String(key)),
    ])));
  }});
  app.mount(host);
  vi.spyOn(list.value!, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 400, 300));
  list.value!.querySelectorAll<HTMLElement>('[data-key]').forEach((row, index) => {
    vi.spyOn(row, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, index * 72, 400, 72));
  });
  const handle = (key: number) => list.value!.querySelector<HTMLButtonElement>(`[data-key="${key}"] button`)!;
  return {keys, visible, list: list.value!, sort, onDrop, handle};
}

describe('list drag sorting', () => {
  it.each(['mouse', 'touch', 'pen'])('previews a stable insertion point and only commits on release (%s)', async pointerType => {
    const {keys, sort, onDrop, handle} = mount();
    handle(0).dispatchEvent(pointer('pointerdown', 36, {pointerType}));
    expect(sort.draggedKey.value).toBe(0);
    for (let step = 0; step < 10; step++) {
      document.dispatchEvent(pointer('pointermove', 250, {pointerType}));
      await nextTick();
      expect(sort.dropIndicator.value).toEqual({key: 2, position: 'after'});
      expect(keys.value).toEqual([0, 1, 2]);
      expect(onDrop).not.toHaveBeenCalled();
    }
    document.dispatchEvent(pointer('pointerup', 250, {pointerType})); await nextTick();
    expect(keys.value).toEqual([1, 2, 0]);
    expect(onDrop).toHaveBeenCalledExactlyOnceWith(0, 2);
    expect(sort.draggedKey.value).toBeNull();
    expect(sort.dropIndicator.value).toBeNull();
    expect(document.body.classList.contains('pad-list-dragging')).toBe(false);
  });

  it('moves the last item before the first and supports numeric key zero', () => {
    const {keys, sort, handle} = mount();
    handle(2).dispatchEvent(pointer('pointerdown', 180));
    document.dispatchEvent(pointer('pointermove', 10));
    expect(sort.dropIndicator.value).toEqual({key: 0, position: 'before'});
    document.dispatchEvent(pointer('pointerup', 10));
    expect(keys.value).toEqual([2, 0, 1]);
  });

  it('does not reorder for a tap or a drop back at the original position', () => {
    const {onDrop, handle} = mount();
    handle(1).dispatchEvent(pointer('pointerdown', 108));
    document.dispatchEvent(pointer('pointerup', 108));
    handle(1).dispatchEvent(pointer('pointerdown', 108));
    document.dispatchEvent(pointer('pointermove', 250));
    document.dispatchEvent(pointer('pointermove', 108));
    document.dispatchEvent(pointer('pointerup', 108));
    expect(onDrop).not.toHaveBeenCalled();
  });

  it.each(['pointercancel', 'lostpointercapture', 'blur', 'Escape', 'hidden', 'unmount', 'changed'])('cancels and removes listeners after %s', async reason => {
    const {keys, visible, sort, onDrop, handle} = mount();
    handle(0).dispatchEvent(pointer('pointerdown', 36));
    document.dispatchEvent(pointer('pointermove', 250));
    if (reason === 'pointercancel') document.dispatchEvent(pointer('pointercancel', 250));
    if (reason === 'lostpointercapture') handle(0).dispatchEvent(new Event('lostpointercapture'));
    if (reason === 'blur') window.dispatchEvent(new Event('blur'));
    if (reason === 'Escape') {
      const event = new KeyboardEvent('keydown', {key: 'Escape', cancelable: true});
      window.dispatchEvent(event); expect(event.defaultPrevented).toBe(true);
    }
    if (reason === 'hidden') visible.value = false;
    if (reason === 'changed') keys.value = [0, 1, 2, 3];
    if (reason === 'unmount') { app!.unmount(); app = undefined; }
    await nextTick();
    document.dispatchEvent(pointer('pointerup', 250));
    expect(onDrop).not.toHaveBeenCalled();
    expect(sort.draggedKey.value).toBeNull();
    expect(document.body.classList.contains('pad-list-dragging')).toBe(false);
  });

  it('ignores right clicks, secondary touches, hidden lists and unrelated pointer events', async () => {
    const {visible, sort, onDrop, handle} = mount();
    handle(0).dispatchEvent(pointer('pointerdown', 36, {button: 2}));
    handle(0).dispatchEvent(pointer('pointerdown', 36, {isPrimary: false}));
    visible.value = false; handle(0).dispatchEvent(pointer('pointerdown', 36));
    expect(sort.draggedKey.value).toBeNull();
    visible.value = true; await nextTick();
    handle(0).dispatchEvent(pointer('pointerdown', 36));
    handle(1).dispatchEvent(pointer('pointerdown', 108, {pointerId: 2}));
    document.dispatchEvent(pointer('pointermove', 250, {pointerId: 2}));
    document.dispatchEvent(pointer('pointerup', 250, {pointerId: 2}));
    expect(sort.draggedKey.value).toBe(0); expect(onDrop).not.toHaveBeenCalled();
    document.dispatchEvent(pointer('pointermove', 250)); document.dispatchEvent(pointer('pointerup', 250));
    expect(onDrop).toHaveBeenCalledExactlyOnceWith(0, 2);
  });

  it('scrolls the list near its edges without mutating the order', () => {
    const {list, keys, handle} = mount(); list.scrollTop = 80;
    handle(0).dispatchEvent(pointer('pointerdown', 36));
    document.dispatchEvent(pointer('pointermove', 290)); expect(list.scrollTop).toBe(92);
    document.dispatchEvent(pointer('pointermove', 5)); expect(list.scrollTop).toBe(80);
    expect(keys.value).toEqual([0, 1, 2]);
  });

  it('does not start a drag for a single item', () => {
    const {sort, handle} = mount([0]);
    handle(0).dispatchEvent(pointer('pointerdown', 36));
    expect(sort.draggedKey.value).toBeNull();
  });
});

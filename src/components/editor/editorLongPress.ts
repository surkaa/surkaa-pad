/** 保留轻触和滚动；仅稳定长按目标时接管系统菜单，表格和工具栏共用。 */
export function installEditorLongPress<T>(
  element: HTMLElement,
  resolve: (target: EventTarget | null) => T | null,
  open: (target: T) => void,
): () => void {
  let pending: {target: T; pointer: number; x: number; y: number} | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let suppressClickUntil = 0;
  let heldPointer: number | null = null;
  function cancel() { clearTimeout(timer); timer = undefined; pending = null; }
  function show() {
    if (!pending) return;
    const {target, pointer} = pending;
    cancel();
    heldPointer = pointer;
    suppressClickUntil = Infinity; // 一直按住时也不能因超时而误触单击。
    open(target);
  }
  function down(event: PointerEvent) {
    cancel();
    if (!event.isPrimary) return;
    heldPointer = null;
    suppressClickUntil = 0; // 新的一次轻触不应被上一次长按吞掉。
    if (event.pointerType !== 'touch' && event.pointerType !== 'pen') return;
    const target = resolve(event.target);
    if (target === null) return;
    pending = {target, pointer: event.pointerId, x: event.clientX, y: event.clientY};
    timer = setTimeout(show, 500);
  }
  function move(event: PointerEvent) {
    if (pending && (event.pointerId !== pending.pointer || Math.hypot(event.clientX - pending.x, event.clientY - pending.y) > 10)) cancel();
  }
  function release(event: PointerEvent) {
    if (event.pointerId === heldPointer) {
      heldPointer = null;
      suppressClickUntil = Date.now() + 1000;
    }
    cancel();
  }
  function context(event: MouseEvent) {
    if (resolve(event.target) === null) return;
    event.preventDefault();
    event.stopPropagation();
    show(); // 某些 WebView 先发 contextmenu，再取消 pointer；避免吞掉这一次长按。
  }
  function click(event: MouseEvent) {
    if (resolve(event.target) === null || Date.now() >= suppressClickUntil) return;
    suppressClickUntil = 0;
    event.preventDefault(); event.stopImmediatePropagation();
  }
  element.addEventListener('pointerdown', down, true);
  element.addEventListener('contextmenu', context, true);
  element.addEventListener('click', click, true);
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', release, true);
  window.addEventListener('pointercancel', release, true);
  window.addEventListener('scroll', cancel, true);
  window.addEventListener('blur', cancel);
  return () => {
    cancel();
    element.removeEventListener('pointerdown', down, true);
    element.removeEventListener('contextmenu', context, true);
    element.removeEventListener('click', click, true);
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', release, true);
    window.removeEventListener('pointercancel', release, true);
    window.removeEventListener('scroll', cancel, true);
    window.removeEventListener('blur', cancel);
  };
}

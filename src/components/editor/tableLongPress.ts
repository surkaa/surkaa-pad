/** 保留轻触编辑和滚动；仅稳定长按单元格时接管系统菜单。 */
export function installTableLongPress<T>(
  element: HTMLElement,
  resolve: (target: EventTarget | null) => T | null,
  open: (target: T) => void,
): () => void {
  let pending: {target: T; pointer: number; x: number; y: number} | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let suppressClickUntil = 0;
  function cancel() { clearTimeout(timer); timer = undefined; pending = null; }
  function show() {
    if (!pending) return;
    const {target} = pending;
    cancel();
    suppressClickUntil = Date.now() + 1000;
    open(target);
  }
  function down(event: PointerEvent) {
    cancel();
    if (!event.isPrimary || (event.pointerType !== 'touch' && event.pointerType !== 'pen')) return;
    const target = resolve(event.target);
    if (target === null) return;
    pending = {target, pointer: event.pointerId, x: event.clientX, y: event.clientY};
    timer = setTimeout(show, 500);
  }
  function move(event: PointerEvent) {
    if (pending && (event.pointerId !== pending.pointer || Math.hypot(event.clientX - pending.x, event.clientY - pending.y) > 10)) cancel();
  }
  function context(event: MouseEvent) {
    if (resolve(event.target) === null) return;
    event.preventDefault();
    event.stopPropagation();
    show(); // 某些 WebView 先发 contextmenu，再取消 pointer；避免吞掉这一次长按。
  }
  function click(event: MouseEvent) {
    if (Date.now() >= suppressClickUntil) return;
    suppressClickUntil = 0;
    event.preventDefault(); event.stopImmediatePropagation();
  }
  element.addEventListener('pointerdown', down, true);
  element.addEventListener('contextmenu', context, true);
  element.addEventListener('click', click, true);
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', cancel, true);
  window.addEventListener('pointercancel', cancel, true);
  window.addEventListener('scroll', cancel, true);
  window.addEventListener('blur', cancel);
  return () => {
    cancel();
    element.removeEventListener('pointerdown', down, true);
    element.removeEventListener('contextmenu', context, true);
    element.removeEventListener('click', click, true);
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', cancel, true);
    window.removeEventListener('pointercancel', cancel, true);
    window.removeEventListener('scroll', cancel, true);
    window.removeEventListener('blur', cancel);
  };
}

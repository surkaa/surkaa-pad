// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {installEditorLongPress} from '../editorLongPress';

let element: HTMLElement;
let cell: HTMLElement;
let dispose: () => void;
let open: ReturnType<typeof vi.fn<(target: HTMLElement) => void>>;
function pointer(type: string, options: PointerEventInit = {}, target: EventTarget = cell) {
  const event = new PointerEvent(type, {bubbles: true, cancelable: true,
    pointerType: 'touch', isPrimary: true, pointerId: 1, clientX: 20, clientY: 20, ...options});
  target.dispatchEvent(event);
  return event;
}
beforeEach(() => {
  vi.useFakeTimers();
  element = document.createElement('div');
  element.innerHTML = '<table><tr><td>单元格</td></tr></table>';
  document.body.append(element); cell = element.querySelector('td')!;
  open = vi.fn();
  dispose = installEditorLongPress(element, target => target === cell ? cell : null, open);
});
afterEach(() => { dispose(); document.body.innerHTML = ''; vi.useRealTimers(); });

describe('mobile editor long press', () => {
  it('opens once and suppresses the followup click and native menu', () => {
    const click = vi.fn(); element.addEventListener('click', click);
    expect(pointer('pointerdown').defaultPrevented).toBe(false);
    vi.advanceTimersByTime(500);
    expect(open).toHaveBeenCalledExactlyOnceWith(cell);
    const native = new MouseEvent('contextmenu', {bubbles: true, cancelable: true});
    cell.dispatchEvent(native); expect(native.defaultPrevented).toBe(true);
    expect(open).toHaveBeenCalledOnce();
    cell.click(); expect(click).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1001); cell.click(); expect(click).toHaveBeenCalledOnce();
  });

  it.each(['pointerup', 'pointercancel', 'scroll', 'blur'])('cancels on %s', event => {
    pointer('pointerdown');
    window.dispatchEvent(new Event(event));
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
  });

  it('does not hijack scrolling or a second finger', () => {
    pointer('pointerdown'); pointer('pointermove', {clientX: 35});
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
    cell.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    expect(open).not.toHaveBeenCalled();
    pointer('pointerdown'); pointer('pointerdown', {pointerId: 2, isPrimary: false});
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
  });

  it('ignores mouse and non-cell presses and disposes pending timers', () => {
    pointer('pointerdown', {pointerType: 'mouse'});
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
    pointer('pointerdown', {}, element);
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
    pointer('pointerdown'); dispose();
    vi.advanceTimersByTime(600); expect(open).not.toHaveBeenCalled();
  });

  it('handles a native context menu arriving before the timer', () => {
    pointer('pointerdown');
    cell.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true}));
    expect(open).toHaveBeenCalledExactlyOnceWith(cell);
    vi.advanceTimersByTime(600); expect(open).toHaveBeenCalledOnce();
  });

  it('suppresses release clicks even after holding for several seconds', () => {
    const click = vi.fn(); element.addEventListener('click', click);
    pointer('pointerdown'); vi.advanceTimersByTime(5000);
    expect(open).toHaveBeenCalledOnce();
    cell.click(); expect(click).not.toHaveBeenCalled();
    pointer('pointerup');
    pointer('pointerdown'); pointer('pointerup'); cell.click();
    expect(click).toHaveBeenCalledOnce();
  });

  it('times the suppression window from release, not from menu opening', () => {
    const click = vi.fn(); element.addEventListener('click', click);
    pointer('pointerdown'); vi.advanceTimersByTime(5000); pointer('pointerup');
    vi.advanceTimersByTime(100); cell.click();
    expect(click).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1001); cell.click();
    expect(click).toHaveBeenCalledOnce();
  });

  it('does not swallow a later tap on a different control in the same container', () => {
    const other = document.createElement('button'); element.append(other);
    const click = vi.fn(); other.addEventListener('click', click);
    pointer('pointerdown'); vi.advanceTimersByTime(500); pointer('pointerup');
    pointer('pointerdown', {}, other); pointer('pointerup', {}, other); other.click();
    expect(click).toHaveBeenCalledOnce();
  });
});

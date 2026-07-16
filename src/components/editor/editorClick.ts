export function shouldFocusEditorEnd(
  target: EventTarget | null,
  wrapper: HTMLElement,
  proseMirror: HTMLElement,
  clientY: number,
): boolean {
  if (target !== wrapper && target !== proseMirror) return false

  const lastNode = proseMirror.lastElementChild
  if (!lastNode) return true

  return clientY >= lastNode.getBoundingClientRect().bottom
}

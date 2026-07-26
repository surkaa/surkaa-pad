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

export function shouldPreventStackedAlbumEditorFocus(
  target: EventTarget | null,
  currentPlatform: string,
): boolean {
  return currentPlatform === 'android'
    && target instanceof Element
    && Boolean(target.closest('.editor-image-album[data-display-mode="stackedCards"]'))
}

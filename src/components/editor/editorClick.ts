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

export function isMobileEditorPlatform(currentPlatform: string): boolean {
  return currentPlatform === 'android' || currentPlatform === 'ios'
}

export function disableMobileImageDragging(
  target: EventTarget | null,
  currentPlatform: string,
): boolean {
  if (!isMobileEditorPlatform(currentPlatform) || !(target instanceof Element)) return false

  const image = target.closest<HTMLImageElement>('img[data-id]')
  if (!image) return false
  image.draggable = false
  const album = image.closest<HTMLElement>('.editor-image-album')
  if (album) album.draggable = false
  return true
}

export function shouldPreventEditorFocus(
  target: EventTarget | null,
  currentPlatform: string,
  albumSelectionActive = false,
): boolean {
  if (!isMobileEditorPlatform(currentPlatform) || !(target instanceof Element)) return false

  if (albumSelectionActive && target.closest('.ProseMirror')) return true

  return Boolean(target.closest(
    '.editor-image-album[data-display-mode="stackedCards"], .editor-summary, .editor-location, .editor-audio-attachment',
  ))
}

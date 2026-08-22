export interface AttachmentNodeMatch {
  type: 'image' | 'video' | 'audio' | 'file'
  attachmentId: string
  el: HTMLElement
}

export function findAttachmentNode(
  el: HTMLElement | null,
  boundary?: HTMLElement,
): AttachmentNodeMatch | null {
  while (el && el !== boundary) {
    const tag = el.tagName.toUpperCase()
    if (tag === 'IMG' && el.dataset.id) {
      return { type: 'image', attachmentId: el.dataset.id, el }
    }
    if (tag === 'VIDEO' && el.dataset.id) {
      return { type: 'video', attachmentId: el.dataset.id, el }
    }
    if (tag === 'AUDIO' && el.dataset.id) {
      return { type: 'audio', attachmentId: el.dataset.id, el }
    }
    if (el.classList.contains('editor-audio-attachment') && el.dataset.id) {
      return { type: 'audio', attachmentId: el.dataset.id, el }
    }
    if (el.classList.contains('editor-file-attachment') && el.dataset.id) {
      return { type: 'file', attachmentId: el.dataset.id, el }
    }
    el = el.parentElement
  }
  return null
}

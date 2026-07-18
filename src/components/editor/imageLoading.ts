const INITIALIZED = 'imageLoadingInitialized'

export function setupEditorImageLoading(root: HTMLElement): () => void {
  const initialize = (image: HTMLImageElement) => {
    if (image.dataset[INITIALIZED] === 'true') return
    image.dataset[INITIALIZED] = 'true'
    image.classList.add('editor-image-loading')

    const finish = () => {
      image.classList.remove('editor-image-loading', 'editor-image-error')
      image.classList.add('editor-image-loaded')
      if (!image.closest('.editor-image-album') && image.naturalWidth > 0) {
        image.style.setProperty('--image-natural-width', `${image.naturalWidth}px`)
      }
    }
    const fail = () => {
      image.classList.remove('editor-image-loading', 'editor-image-loaded')
      image.classList.add('editor-image-error')
    }

    image.addEventListener('load', finish)
    image.addEventListener('error', fail)
    if (image.complete) {
      image.naturalWidth > 0 ? finish() : fail()
    }
  }

  const initializeTree = (node: Node) => {
    if (!(node instanceof HTMLElement)) return
    if (node instanceof HTMLImageElement && node.matches('img[data-id]')) initialize(node)
    node.querySelectorAll<HTMLImageElement>('img[data-id]').forEach(initialize)
  }

  initializeTree(root)
  const observer = new MutationObserver(records => {
    records.forEach(record => record.addedNodes.forEach(initializeTree))
  })
  observer.observe(root, { childList: true, subtree: true })
  return () => observer.disconnect()
}

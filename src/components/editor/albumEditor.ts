interface EditorJsonNode {
  type: string
  attrs?: Record<string, unknown>
  content?: EditorJsonNode[]
  [key: string]: unknown
}

export function createAlbumDocument(
  document: EditorJsonNode,
  selectedImages: string[],
  anchorFilename: string,
  albumId: string,
  displayMode: 'horizontalList' | 'stackedCards',
  attachmentMap: Record<string, string>,
): EditorJsonNode {
  const selected = new Set(selectedImages)
  const content = document.content || []
  const nextContent: EditorJsonNode[] = []
  let inserted = false

  for (const node of content) {
    const filename = node.type === 'imageNode' ? node.attrs?.id : undefined
    if (typeof filename === 'string' && selected.has(filename)) {
      if (filename === anchorFilename && !inserted) {
        nextContent.push({
          type: 'albumNode',
          attrs: {
            id: albumId,
            images: selectedImages,
            displayMode,
            urls: selectedImages.map(image => attachmentMap[image] || ''),
          },
        })
        inserted = true
      }
      continue
    }
    nextContent.push(node)
  }

  return inserted ? { ...document, content: nextContent } : document
}

export function changeAlbumDisplayMode(
  document: EditorJsonNode,
  albumId: string,
  displayMode: 'horizontalList' | 'stackedCards',
): EditorJsonNode {
  const content = document.content || []
  return {
    ...document,
    content: content.map(node => {
      if (node.type !== 'albumNode' || node.attrs?.id !== albumId) return node
      return {
        ...node,
        attrs: {
          ...node.attrs,
          displayMode,
        },
      }
    }),
  }
}

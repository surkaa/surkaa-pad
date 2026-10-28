export interface EditorJsonNode {
  type: string
  attrs?: Record<string, unknown>
  content?: EditorJsonNode[]
  [key: string]: unknown
}

export interface AlbumSummary {
  id: string
  images: string[]
  urls: string[]
  displayMode: 'horizontalList' | 'stackedCards'
}

export type AlbumSplitOperation =
  | { type: 'all' }
  | { type: 'single'; position: 'before' | 'after' }
  | { type: 'range'; direction: 'before' | 'after' }

export function createAlbumDocument(
  document: EditorJsonNode,
  selectedImages: string[],
  anchorFilename: string,
  albumId: string,
  displayMode: 'horizontalList' | 'stackedCards',
  attachmentMap: Record<string, string>,
): EditorJsonNode {
  const localImages = (document.content || []).filter(node => node.type === 'imageNode').map(node => node.attrs?.id);
  if (!localImages.includes(anchorFilename)) return mapSummaryScopes(document, body =>
    createAlbumDocument(body, selectedImages, anchorFilename, albumId, displayMode, attachmentMap));
  if (!selectedImages.every(id => localImages.includes(id))) return document;
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
  document = mapSummaryScopes(document, body => changeAlbumDisplayMode(body, albumId, displayMode));
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

export function listAlbums(document: EditorJsonNode): AlbumSummary[] {
  return scopeNodes(document).flatMap(node => {
    if (node.type !== 'albumNode' || typeof node.attrs?.id !== 'string') return []
    const images = stringArray(node.attrs.images)
    if (images.length === 0) return []
    const displayMode = node.attrs.displayMode === 'stackedCards'
      ? 'stackedCards'
      : 'horizontalList'
    return [{
      id: node.attrs.id,
      images,
      urls: stringArray(node.attrs.urls),
      displayMode,
    }]
  })
}

export function addImageToAlbumDocument(
  document: EditorJsonNode,
  filename: string,
  albumId: string,
  insertionIndex: number,
  attachmentMap: Record<string, string>,
): EditorJsonNode {
  const content = scopeNodes(document)
  const source = content.find(node =>
    node.type === 'imageNode' && node.attrs?.id === filename,
  )
  const target = content.find(node =>
    node.type === 'albumNode' && node.attrs?.id === albumId,
  )
  const targetImages = stringArray(target?.attrs?.images)

  if (
    !source
    || !target
    || !Number.isInteger(insertionIndex)
    || insertionIndex < 0
    || insertionIndex > targetImages.length
    || targetImages.includes(filename)
  ) {
    return document
  }

  const targetUrls = stringArray(target.attrs?.urls)
  const sourceUrl = attachmentMap[filename]
    || (typeof source.attrs?.src === 'string' ? source.attrs.src : '')
  const images = [...targetImages]
  const urls = targetImages.map((_, index) => targetUrls[index] || '')
  images.splice(insertionIndex, 0, filename)
  urls.splice(insertionIndex, 0, sourceUrl)

  const update = (body: EditorJsonNode): EditorJsonNode => ({
    ...body, content: (body.content || []).filter(node => node !== source).map(node => node === target
      ? {...node, attrs: {...node.attrs, images, urls}} : node),
  });
  return update(mapSummaryScopes(document, update));
}

export function splitAlbumDocument(
  document: EditorJsonNode,
  albumId: string,
  selectedFilename: string,
  operation: AlbumSplitOperation,
): EditorJsonNode {
  const content = document.content || []
  const targetIndex = content.findIndex(node =>
    node.type === 'albumNode' && node.attrs?.id === albumId,
  )
  if (targetIndex < 0) return mapSummaryScopes(document, body => splitAlbumDocument(body, albumId, selectedFilename, operation))

  const album = content[targetIndex]
  const images = stringArray(album.attrs?.images)
  const urls = stringArray(album.attrs?.urls)
  const selectedIndex = images.indexOf(selectedFilename)
  if (images.length === 0 || (operation.type !== 'all' && selectedIndex < 0)) {
    return document
  }

  const imageNodes = (start: number, end: number) => images
    .slice(start, end)
    .map((filename, offset) => createImageNode(filename, urls[start + offset] || ''))

  const albumOrImages = (start: number, end: number): EditorJsonNode[] => {
    const remainingImages = images.slice(start, end)
    const remainingUrls = remainingImages.map((_, offset) => urls[start + offset] || '')
    if (remainingImages.length >= 2) {
      return [{
        ...album,
        attrs: {
          ...album.attrs,
          images: remainingImages,
          urls: remainingUrls,
        },
      }]
    }
    return remainingImages.map((filename, index) =>
      createImageNode(filename, remainingUrls[index] || ''),
    )
  }

  let replacement: EditorJsonNode[]
  if (operation.type === 'all') {
    replacement = imageNodes(0, images.length)
  } else if (operation.type === 'single') {
    const selected = imageNodes(selectedIndex, selectedIndex + 1)
    const remainingImages = images.filter((_, index) => index !== selectedIndex)
    const remainingUrls = urls.filter((_, index) => index !== selectedIndex)
    const remaining = remainingImages.length >= 2
      ? [{
          ...album,
          attrs: { ...album.attrs, images: remainingImages, urls: remainingUrls },
        }]
      : remainingImages.map((filename, index) =>
          createImageNode(filename, remainingUrls[index] || ''),
        )
    replacement = operation.position === 'before'
      ? [...selected, ...remaining]
      : [...remaining, ...selected]
  } else if (operation.direction === 'before') {
    replacement = [
      ...imageNodes(0, selectedIndex + 1),
      ...albumOrImages(selectedIndex + 1, images.length),
    ]
  } else {
    replacement = [
      ...albumOrImages(0, selectedIndex),
      ...imageNodes(selectedIndex, images.length),
    ]
  }

  return {
    ...document,
    content: [
      ...content.slice(0, targetIndex),
      ...replacement,
      ...content.slice(targetIndex + 1),
    ],
  }
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter(item => typeof item === 'string') : []
}

function createImageNode(filename: string, src: string): EditorJsonNode {
  return {
    type: 'imageNode',
    attrs: { id: filename, src },
  }
}
/** 日记顶层与折叠正文是独立的块作用域；不把内外图片误合并。 */
function mapSummaryScopes(document: EditorJsonNode, operation: (body: EditorJsonNode) => EditorJsonNode): EditorJsonNode {
  let changed = false;
  const content = (document.content || []).map(node => {
    if (node.type !== 'summaryNode') return node;
    const children = (node.content || []).map(child => {
      if (child.type !== 'summaryBody') return child;
      const next = operation(child); if (next !== child) changed = true;
      return next;
    });
    return children.some((child, i) => child !== node.content?.[i]) ? {...node, content: children} : node;
  });
  return changed ? {...document, content} : document;
}

function scopeNodes(document: EditorJsonNode): EditorJsonNode[] {
  return (document.content || []).flatMap(node => node.type === 'summaryNode'
    ? (node.content?.find(child => child.type === 'summaryBody')?.content || []) : [node]);
}

export type AttachmentNodeKind = 'image' | 'audio' | 'video' | 'file'

export interface UploadedAttachment {
  nodeKind: AttachmentNodeKind
  attachmentId: string
  filename: string
  url: string
}

export type AttachmentInsertion =
  | { type: 'image' | 'audio' | 'video'; attachmentId: string; url: string }
  | { type: 'file'; attachmentId: string; filename: string; url: string }
  | { type: 'album'; id: string; images: string[]; urls: string[] }

export interface AttachmentInsertionTarget {
  insertAttachments(insertions: readonly AttachmentInsertion[], atEnd: boolean): boolean
}

export interface AttachmentEditorNode {
  type: string
  attrs: Record<string, unknown>
}

export function attachmentNodeKindFromMimeType(mimetype: string): AttachmentNodeKind {
  const normalized = mimetype.toLowerCase()
  if (normalized.startsWith('image/')) return 'image'
  if (normalized.startsWith('audio/')) return 'audio'
  if (normalized.startsWith('video/')) return 'video'
  return 'file'
}

export function attachmentInsertionsToEditorContent(
  insertions: readonly AttachmentInsertion[],
): AttachmentEditorNode[] {
  return insertions.map(insertion => {
    switch (insertion.type) {
      case 'image':
        return {type: 'imageNode', attrs: {id: insertion.attachmentId, src: insertion.url}}
      case 'audio':
        return {type: 'audioNode', attrs: {id: insertion.attachmentId, src: insertion.url}}
      case 'video':
        return {type: 'videoNode', attrs: {id: insertion.attachmentId, src: insertion.url}}
      case 'file':
        return {type: 'fileNode', attrs: {id: insertion.attachmentId, filename: insertion.filename}}
      case 'album':
        return {
          type: 'albumNode',
          attrs: {
            id: insertion.id,
            images: insertion.images,
            urls: insertion.urls,
            displayMode: 'horizontalList',
            hasCycled: false,
          },
        }
    }
  })
}

/**
 * 将上传结果转换成编辑器插入计划。失败项由调用方过滤为 null；相邻的多张图片合并为图集。
 */
export function planAttachmentInsertions(
  results: readonly (UploadedAttachment | null)[],
  createAlbumId: () => string,
): AttachmentInsertion[] {
  const successful = results.filter((item): item is UploadedAttachment => item !== null)
  const insertions: AttachmentInsertion[] = []

  for (let index = 0; index < successful.length;) {
    const item = successful[index]
    if (item.nodeKind !== 'image') {
      insertions.push(item.nodeKind === 'file'
        ? { type: 'file', attachmentId: item.attachmentId, filename: item.filename, url: item.url }
        : { type: item.nodeKind, attachmentId: item.attachmentId, url: item.url })
      index += 1
      continue
    }

    const images: UploadedAttachment[] = []
    while (successful[index]?.nodeKind === 'image') {
      images.push(successful[index])
      index += 1
    }

    if (images.length === 1) {
      insertions.push({ type: 'image', attachmentId: images[0].attachmentId, url: images[0].url })
    } else {
      insertions.push({
        type: 'album',
        id: createAlbumId(),
        images: images.map(image => image.attachmentId),
        urls: images.map(image => image.url),
      })
    }
  }

  return insertions
}

export async function applyAttachmentInsertions(
  insertions: readonly AttachmentInsertion[],
  target: AttachmentInsertionTarget,
  atEnd = false,
): Promise<boolean> {
  if (!insertions.length) return true
  return target.insertAttachments(insertions, atEnd)
}

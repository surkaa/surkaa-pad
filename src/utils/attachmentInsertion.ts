export type AttachmentNodeKind = 'image' | 'audio' | 'video' | 'file'

export interface UploadedAttachment {
  nodeKind: AttachmentNodeKind
  filename: string
  url: string
}

export type AttachmentInsertion =
  | { type: 'image' | 'audio' | 'video' | 'file'; filename: string; url: string }
  | { type: 'album'; id: string; images: string[]; urls: string[] }

export interface AttachmentInsertionTarget {
  insertImage(filename: string): void
  insertAudio(filename: string): void
  insertVideo(filename: string): void
  insertFile(filename: string): void
  insertAlbum(id: string, images: string[], urls: string[]): void
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
      insertions.push({ type: item.nodeKind, filename: item.filename, url: item.url })
      index += 1
      continue
    }

    const images: UploadedAttachment[] = []
    while (successful[index]?.nodeKind === 'image') {
      images.push(successful[index])
      index += 1
    }

    if (images.length === 1) {
      insertions.push({ type: 'image', filename: images[0].filename, url: images[0].url })
    } else {
      insertions.push({
        type: 'album',
        id: createAlbumId(),
        images: images.map(image => image.filename),
        urls: images.map(image => image.url),
      })
    }
  }

  return insertions
}

export async function applyAttachmentInsertions(
  insertions: readonly AttachmentInsertion[],
  target: AttachmentInsertionTarget,
  afterEach: () => Promise<void> = async () => {},
): Promise<void> {
  for (const insertion of insertions) {
    switch (insertion.type) {
      case 'image': target.insertImage(insertion.filename); break
      case 'audio': target.insertAudio(insertion.filename); break
      case 'video': target.insertVideo(insertion.filename); break
      case 'file': target.insertFile(insertion.filename); break
      case 'album': target.insertAlbum(insertion.id, insertion.images, insertion.urls); break
    }
    await afterEach()
  }
}

import { describe, expect, it } from 'vitest'
import type { AttachmentMeta, DiaryContent } from '../../bindings'
import {
  collectReferencedAttachmentIds,
  findUnusedAttachments,
} from '../diaryAttachments'

function attachment(id: string, filename = `${id}.bin`): AttachmentMeta {
  return {
    id,
    filename,
    mimetype: 'application/octet-stream',
    size: 1,
    encrypted: false,
    nonce: [],
    algorithm: 'AES-256-CTR',
    etag: null,
  }
}

describe('diary attachment references', () => {
  it('collects stable IDs from single nodes and albums', () => {
    const content: DiaryContent = {
      nodes: [
        { type: 'markdown', text: '正文' },
        { type: 'image', attachmentId: 'image-1', size: 'normal' },
        { type: 'audio', attachmentId: 'audio-1' },
        {
          type: 'album',
          id: 'album-1',
          attachmentIds: ['image-2', 'image-3', 'image-2'],
          displayMode: 'horizontalList',
        },
      ],
    }

    expect([...collectReferencedAttachmentIds(content)]).toEqual([
      'image-1',
      'audio-1',
      'image-2',
      'image-3',
    ])
  })

  it('finds metadata not referenced by content without comparing filenames', () => {
    const content: DiaryContent = {
      nodes: [{ type: 'file', attachmentId: 'file-1' }],
    }
    const renamed = attachment('file-1', 'renamed.txt')
    const unused = attachment('file-2', 'renamed.txt')

    expect(findUnusedAttachments(content, [renamed, unused])).toEqual([unused])
  })

  it('treats an empty document as referencing no attachments', () => {
    const attachments = [attachment('a'), attachment('b')]
    expect(findUnusedAttachments({ nodes: [] }, attachments)).toEqual(attachments)
  })
})

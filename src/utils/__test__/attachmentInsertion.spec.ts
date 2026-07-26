import { describe, expect, it, vi } from 'vitest'
import {
  attachmentNodeKindFromMimeType,
  attachmentInsertionsToEditorContent,
  applyAttachmentInsertions,
  planAttachmentInsertions,
  type AttachmentInsertion,
  type AttachmentInsertionTarget,
  type UploadedAttachment,
} from '../attachmentInsertion'

const image = (filename: string): UploadedAttachment => ({
  nodeKind: 'image', attachmentId: `att-${filename}`, filename, url: `url://${filename}`,
})

describe('planAttachmentInsertions', () => {
  it('把一次上传的多张图片合并为一个图集，并保持选择顺序', () => {
    expect(planAttachmentInsertions(
      [image('2.jpg'), image('1.jpg'), image('3.jpg')],
      () => 'album-1',
    )).toEqual([{
      type: 'album',
      id: 'album-1',
      images: ['att-2.jpg', 'att-1.jpg', 'att-3.jpg'],
      urls: ['url://2.jpg', 'url://1.jpg', 'url://3.jpg'],
    }])
  })

  it('单张成功图片仍插入普通图片', () => {
    expect(planAttachmentInsertions([image('one.jpg')], vi.fn())).toEqual([
      { type: 'image', attachmentId: 'att-one.jpg', url: 'url://one.jpg' },
    ])
  })

  it('忽略失败项，剩余至少两张图片时仍创建图集', () => {
    expect(planAttachmentInsertions(
      [image('1.jpg'), null, image('2.jpg')],
      () => 'album-after-failure',
    )).toEqual([{
      type: 'album',
      id: 'album-after-failure',
      images: ['att-1.jpg', 'att-2.jpg'],
      urls: ['url://1.jpg', 'url://2.jpg'],
    }])
  })

  it('非图片中断图片分组，并保持所有附件的插入顺序', () => {
    const audio: UploadedAttachment = { nodeKind: 'audio', attachmentId: 'att-audio', filename: 'a.mp3', url: 'url://a' }
    const file: UploadedAttachment = { nodeKind: 'file', attachmentId: 'att-file', filename: 'a.pdf', url: 'url://f' }
    let id = 0
    expect(planAttachmentInsertions(
      [image('1.jpg'), image('2.jpg'), audio, image('3.jpg'), file, image('4.jpg'), image('5.jpg')],
      () => `album-${++id}`,
    )).toEqual([
      { type: 'album', id: 'album-1', images: ['att-1.jpg', 'att-2.jpg'], urls: ['url://1.jpg', 'url://2.jpg'] },
      { type: 'audio', attachmentId: 'att-audio', url: 'url://a' },
      { type: 'image', attachmentId: 'att-3.jpg', url: 'url://3.jpg' },
      { type: 'file', attachmentId: 'att-file', filename: 'a.pdf', url: 'url://f' },
      { type: 'album', id: 'album-2', images: ['att-4.jpg', 'att-5.jpg'], urls: ['url://4.jpg', 'url://5.jpg'] },
    ])
  })

  it('全失败或空输入不创建 ID，也不产生插入项', () => {
    const createAlbumId = vi.fn(() => 'unused')
    expect(planAttachmentInsertions([null, null], createAlbumId)).toEqual([])
    expect(planAttachmentInsertions([], createAlbumId)).toEqual([])
    expect(createAlbumId).not.toHaveBeenCalled()
  })
})

describe('applyAttachmentInsertions', () => {
  it('一次性把整个计划交给编辑器，避免连续原子节点互相替换', async () => {
    const insertAttachments = vi.fn(() => true)
    const target: AttachmentInsertionTarget = {
      insertAttachments,
    }
    const insertions: AttachmentInsertion[] = [
      { type: 'album', id: 'a1', images: ['1.jpg', '2.jpg'], urls: ['u1', 'u2'] },
      { type: 'video', attachmentId: 'att-video', url: 'uv' },
      { type: 'file', attachmentId: 'att-file', filename: 'f.pdf', url: 'uf' },
    ]

    await expect(applyAttachmentInsertions(insertions, target, true)).resolves.toBe(true)
    expect(insertAttachments).toHaveBeenCalledOnce()
    expect(insertAttachments).toHaveBeenCalledWith(insertions, true)
  })

  it('空计划不调用编辑器', async () => {
    const insertAttachments = vi.fn(() => true)
    await expect(applyAttachmentInsertions([], {insertAttachments}, true)).resolves.toBe(true)
    expect(insertAttachments).not.toHaveBeenCalled()
  })
})

describe('attachmentNodeKindFromMimeType', () => {
  it.each([
    ['image/jpeg', 'image'],
    ['AUDIO/MP4', 'audio'],
    ['video/mp4', 'video'],
    ['application/pdf', 'file'],
    ['', 'file'],
  ] as const)('将 %s 识别为 %s 节点', (mimetype, expected) => {
    expect(attachmentNodeKindFromMimeType(mimetype)).toBe(expected)
  })
})

describe('attachmentInsertionsToEditorContent', () => {
  it('将多个视频保留为同一次插入中的多个独立节点', () => {
    expect(attachmentInsertionsToEditorContent([
      {type: 'video', attachmentId: 'video-1', url: 'url://1'},
      {type: 'video', attachmentId: 'video-2', url: 'url://2'},
      {type: 'video', attachmentId: 'video-3', url: 'url://3'},
    ])).toEqual([
      {type: 'videoNode', attrs: {id: 'video-1', src: 'url://1'}},
      {type: 'videoNode', attrs: {id: 'video-2', src: 'url://2'}},
      {type: 'videoNode', attrs: {id: 'video-3', src: 'url://3'}},
    ])
  })
})

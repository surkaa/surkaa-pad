import { describe, expect, it, vi } from 'vitest'
import {
  applyAttachmentInsertions,
  planAttachmentInsertions,
  type AttachmentInsertionTarget,
  type UploadedAttachment,
} from '../attachmentInsertion'

const image = (filename: string): UploadedAttachment => ({
  nodeKind: 'image', filename, url: `url://${filename}`,
})

describe('planAttachmentInsertions', () => {
  it('把一次上传的多张图片合并为一个图集，并保持选择顺序', () => {
    expect(planAttachmentInsertions(
      [image('2.jpg'), image('1.jpg'), image('3.jpg')],
      () => 'album-1',
    )).toEqual([{
      type: 'album',
      id: 'album-1',
      images: ['2.jpg', '1.jpg', '3.jpg'],
      urls: ['url://2.jpg', 'url://1.jpg', 'url://3.jpg'],
    }])
  })

  it('单张成功图片仍插入普通图片', () => {
    expect(planAttachmentInsertions([image('one.jpg')], vi.fn())).toEqual([
      { type: 'image', filename: 'one.jpg', url: 'url://one.jpg' },
    ])
  })

  it('忽略失败项，剩余至少两张图片时仍创建图集', () => {
    expect(planAttachmentInsertions(
      [image('1.jpg'), null, image('2.jpg')],
      () => 'album-after-failure',
    )).toEqual([{
      type: 'album',
      id: 'album-after-failure',
      images: ['1.jpg', '2.jpg'],
      urls: ['url://1.jpg', 'url://2.jpg'],
    }])
  })

  it('非图片中断图片分组，并保持所有附件的插入顺序', () => {
    const audio: UploadedAttachment = { nodeKind: 'audio', filename: 'a.mp3', url: 'url://a' }
    const file: UploadedAttachment = { nodeKind: 'file', filename: 'a.pdf', url: 'url://f' }
    let id = 0
    expect(planAttachmentInsertions(
      [image('1.jpg'), image('2.jpg'), audio, image('3.jpg'), file, image('4.jpg'), image('5.jpg')],
      () => `album-${++id}`,
    )).toEqual([
      { type: 'album', id: 'album-1', images: ['1.jpg', '2.jpg'], urls: ['url://1.jpg', 'url://2.jpg'] },
      { type: 'audio', filename: 'a.mp3', url: 'url://a' },
      { type: 'image', filename: '3.jpg', url: 'url://3.jpg' },
      { type: 'file', filename: 'a.pdf', url: 'url://f' },
      { type: 'album', id: 'album-2', images: ['4.jpg', '5.jpg'], urls: ['url://4.jpg', 'url://5.jpg'] },
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
  it('仅负责按计划调用编辑器，并在每次插入后等待刷新', async () => {
    const calls: string[] = []
    const target: AttachmentInsertionTarget = {
      insertImage: filename => calls.push(`image:${filename}`),
      insertAudio: filename => calls.push(`audio:${filename}`),
      insertVideo: filename => calls.push(`video:${filename}`),
      insertFile: filename => calls.push(`file:${filename}`),
      insertAlbum: (id, images) => calls.push(`album:${id}:${images.join(',')}`),
    }
    const afterEach = vi.fn(async () => { calls.push('tick') })

    await applyAttachmentInsertions([
      { type: 'album', id: 'a1', images: ['1.jpg', '2.jpg'], urls: ['u1', 'u2'] },
      { type: 'video', filename: 'v.mp4', url: 'uv' },
      { type: 'file', filename: 'f.pdf', url: 'uf' },
    ], target, afterEach)

    expect(calls).toEqual(['album:a1:1.jpg,2.jpg', 'tick', 'video:v.mp4', 'tick', 'file:f.pdf', 'tick'])
    expect(afterEach).toHaveBeenCalledTimes(3)
  })
})

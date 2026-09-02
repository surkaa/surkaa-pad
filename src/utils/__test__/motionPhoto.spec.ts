import {describe, expect, it} from 'vitest'
import {
  buildMotionPhotoVideoUrl,
  MOTION_PHOTO_VIDEO_VIEW,
} from '../motionPhoto'

describe('Motion Photo 虚拟视频 URL', () => {
  it('保留附件鉴权和缓存参数并添加媒体视图', () => {
    const result = buildMotionPhotoVideoUrl(
      'http://127.0.0.1:63993/token/diary/attachment?t=123',
    )

    expect(result).toBe(
      `http://127.0.0.1:63993/token/diary/attachment?t=123&view=${MOTION_PHOTO_VIDEO_VIEW}`,
    )
  })

  it('覆盖已有的媒体视图而不产生重复参数', () => {
    const result = buildMotionPhotoVideoUrl(
      'http://127.0.0.1/token/diary/attachment?view=original&view=duplicate',
    )
    const url = new URL(result!)

    expect(url.searchParams.getAll('view')).toEqual([MOTION_PHOTO_VIDEO_VIEW])
  })

  it('拒绝无法解析的附件 URL', () => {
    expect(buildMotionPhotoVideoUrl('not a url')).toBeNull()
  })
})

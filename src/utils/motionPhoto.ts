export const MOTION_PHOTO_VIDEO_VIEW = 'motion-photo-video'

/**
 * 将附件 URL 映射为后端提供的动态照片内嵌视频资源。
 * 原有鉴权 token 和缓存参数会被完整保留。
 */
export function buildMotionPhotoVideoUrl(attachmentUrl: string): string | null {
  try {
    const url = new URL(attachmentUrl)
    url.searchParams.set('view', MOTION_PHOTO_VIDEO_VIEW)
    return url.toString()
  } catch {
    return null
  }
}

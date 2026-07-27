export function animateStackedAlbumCycle(
  album: HTMLElement,
  onCycle: () => void,
  duration = 300,
): boolean {
  if (album.dataset.animating === 'true') return false

  const images = album.querySelectorAll('img[data-id]')
  const currentImage = images[0] as HTMLElement | undefined
  const nextImage = images[1] as HTMLElement | undefined
  if (!currentImage || !nextImage) return false

  album.dataset.animating = 'true'
  album.classList.add('album-cycling')

  window.setTimeout(() => {
    // NodeView 会复用并重新排列图片 DOM。先结束旧顺序的动画状态，再在同一
    // 任务中更新节点，确保居中的下一张和移到左侧的上一张保持当前视觉位置。
    album.classList.remove('album-cycling')
    onCycle()
    window.requestAnimationFrame(() => {
      delete album.dataset.animating
    })
  }, duration)

  return true
}

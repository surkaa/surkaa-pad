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
    onCycle()
    window.requestAnimationFrame(() => {
      album.classList.remove('album-cycling')
      delete album.dataset.animating
    })
  }, duration)

  return true
}

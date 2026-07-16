// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { animateStackedAlbumCycle } from '../albumAnimation'

describe('animateStackedAlbumCycle', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('animates the top image before cycling and unlocks afterwards', () => {
    vi.useFakeTimers()
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation(callback => {
      callback(0)
      return 1
    })
    const album = document.createElement('div')
    const currentImage = document.createElement('img')
    currentImage.dataset.id = '1.jpg'
    const nextImage = document.createElement('img')
    nextImage.dataset.id = '2.jpg'
    album.append(currentImage, nextImage)
    const onCycle = vi.fn()

    expect(animateStackedAlbumCycle(album, onCycle, 300)).toBe(true)
    expect(album.classList.contains('album-cycling')).toBe(true)
    expect(album.dataset.animating).toBe('true')

    vi.advanceTimersByTime(300)

    expect(onCycle).toHaveBeenCalledOnce()
    expect(album.classList.contains('album-cycling')).toBe(false)
    expect(album.dataset.animating).toBeUndefined()
  })

  it('ignores repeated clicks while an animation is running', () => {
    vi.useFakeTimers()
    const album = document.createElement('div')
    const currentImage = document.createElement('img')
    currentImage.dataset.id = '1.jpg'
    const nextImage = document.createElement('img')
    nextImage.dataset.id = '2.jpg'
    album.append(currentImage, nextImage)

    expect(animateStackedAlbumCycle(album, vi.fn())).toBe(true)
    expect(animateStackedAlbumCycle(album, vi.fn())).toBe(false)
  })
})

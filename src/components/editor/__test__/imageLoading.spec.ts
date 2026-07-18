// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { setupEditorImageLoading } from '../imageLoading'

function setImageState(image: HTMLImageElement, complete: boolean, naturalWidth: number) {
  Object.defineProperty(image, 'complete', { configurable: true, value: complete })
  Object.defineProperty(image, 'naturalWidth', { configurable: true, value: naturalWidth })
}

describe('setupEditorImageLoading', () => {
  it('marks an incomplete image as loading, then uses its intrinsic width', () => {
    const root = document.createElement('div')
    const image = document.createElement('img')
    image.dataset.id = 'small-source.jpg'
    setImageState(image, false, 0)
    root.append(image)

    setupEditorImageLoading(root)
    expect(image.classList.contains('editor-image-loading')).toBe(true)

    setImageState(image, true, 100)
    image.dispatchEvent(new Event('load'))
    expect(image.classList.contains('editor-image-loaded')).toBe(true)
    expect(image.style.getPropertyValue('--image-natural-width')).toBe('100px')
  })

  it('keeps album image sizing controlled by the album layout', () => {
    const root = document.createElement('div')
    const album = document.createElement('div')
    album.className = 'editor-image-album'
    const image = document.createElement('img')
    image.dataset.id = 'album.jpg'
    setImageState(image, true, 240)
    album.append(image)
    root.append(album)

    setupEditorImageLoading(root)
    expect(image.classList.contains('editor-image-loaded')).toBe(true)
    expect(image.style.getPropertyValue('--image-natural-width')).toBe('')
  })

  it('marks failed images and initializes images inserted later', async () => {
    const root = document.createElement('div')
    setupEditorImageLoading(root)
    const image = document.createElement('img')
    image.dataset.id = 'late.jpg'
    setImageState(image, false, 0)
    root.append(image)
    await Promise.resolve()

    expect(image.classList.contains('editor-image-loading')).toBe(true)
    image.dispatchEvent(new Event('error'))
    expect(image.classList.contains('editor-image-error')).toBe(true)
  })
})

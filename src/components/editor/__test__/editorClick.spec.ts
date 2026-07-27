// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest'
import {
  isMobileEditorPlatform,
  shouldFocusEditorEnd,
  shouldPreventEditorFocus,
} from '../editorClick'

describe('shouldFocusEditorEnd', () => {
  it('focuses the end when clicking below the final document node', () => {
    const wrapper = document.createElement('div')
    const proseMirror = document.createElement('div')
    const lastNode = document.createElement('p')
    proseMirror.append(lastNode)
    wrapper.append(proseMirror)
    vi.spyOn(lastNode, 'getBoundingClientRect').mockReturnValue({
      bottom: 200,
    } as DOMRect)

    expect(shouldFocusEditorEnd(proseMirror, wrapper, proseMirror, 240)).toBe(true)
  })

  it('does not focus the end when clicking a gap before the final node', () => {
    const wrapper = document.createElement('div')
    const proseMirror = document.createElement('div')
    const lastNode = document.createElement('img')
    proseMirror.append(lastNode)
    wrapper.append(proseMirror)
    vi.spyOn(lastNode, 'getBoundingClientRect').mockReturnValue({
      bottom: 300,
    } as DOMRect)

    expect(shouldFocusEditorEnd(proseMirror, wrapper, proseMirror, 180)).toBe(false)
  })

  it('focuses an empty editor and ignores clicks on document nodes', () => {
    const wrapper = document.createElement('div')
    const proseMirror = document.createElement('div')
    const paragraph = document.createElement('p')
    wrapper.append(proseMirror)

    expect(shouldFocusEditorEnd(proseMirror, wrapper, proseMirror, 0)).toBe(true)
    expect(shouldFocusEditorEnd(paragraph, wrapper, proseMirror, 500)).toBe(false)
  })
})

describe('shouldPreventEditorFocus', () => {
  it('prevents Android interactions inside a stacked album from focusing the editor', () => {
    const album = document.createElement('div')
    album.className = 'editor-image-album'
    album.dataset.displayMode = 'stackedCards'
    const image = document.createElement('img')
    album.append(image)

    expect(shouldPreventEditorFocus(image, 'android')).toBe(true)
    expect(shouldPreventEditorFocus(album, 'android')).toBe(true)
  })

  it('keeps desktop and horizontal album interactions unchanged', () => {
    const album = document.createElement('div')
    album.className = 'editor-image-album'
    album.dataset.displayMode = 'horizontalList'
    const image = document.createElement('img')
    album.append(image)

    expect(shouldPreventEditorFocus(image, 'android')).toBe(false)
    album.dataset.displayMode = 'stackedCards'
    expect(shouldPreventEditorFocus(image, 'windows')).toBe(false)
    expect(shouldPreventEditorFocus(null, 'android')).toBe(false)
  })

  it('prevents editor focus throughout mobile album image selection', () => {
    const proseMirror = document.createElement('div')
    proseMirror.className = 'ProseMirror'
    const image = document.createElement('img')
    proseMirror.append(image)

    expect(shouldPreventEditorFocus(image, 'android', true)).toBe(true)
    expect(shouldPreventEditorFocus(proseMirror, 'ios', true)).toBe(true)
    expect(shouldPreventEditorFocus(image, 'windows', true)).toBe(false)
    expect(shouldPreventEditorFocus(image, 'android', false)).toBe(false)
  })

  it('does not block controls outside the editor during album selection', () => {
    const button = document.createElement('button')

    expect(shouldPreventEditorFocus(button, 'android', true)).toBe(false)
  })
})

describe('isMobileEditorPlatform', () => {
  it('recognizes mobile editor platforms', () => {
    expect(isMobileEditorPlatform('android')).toBe(true)
    expect(isMobileEditorPlatform('ios')).toBe(true)
    expect(isMobileEditorPlatform('windows')).toBe(false)
  })
})

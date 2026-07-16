// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest'
import { shouldFocusEditorEnd } from '../editorClick'

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

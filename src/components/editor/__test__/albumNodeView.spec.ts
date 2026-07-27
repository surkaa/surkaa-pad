// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { Schema } from '@tiptap/pm/model'
import { DecorationSet } from '@tiptap/pm/view'
import { createAlbumNodeView } from '../tiptap-extensions/AlbumNode'

const schema = new Schema({
  nodes: {
    doc: { content: 'albumNode*' },
    text: { group: 'inline' },
    albumNode: {
      group: 'block',
      atom: true,
      attrs: {
        id: { default: null },
        images: { default: [] },
        urls: { default: [] },
        displayMode: { default: 'stackedCards' },
        hasCycled: { default: false },
      },
    },
  },
})

function albumNode(images: string[], urls: string[], hasCycled = false) {
  return schema.nodes.albumNode.create({
    id: 'album-1',
    images,
    urls,
    displayMode: 'stackedCards',
    hasCycled,
  })
}

describe('album node view', () => {
  it('reuses image elements and their sources when cycling', () => {
    const view = createAlbumNodeView(albumNode(
      ['att-1', 'att-2', 'att-3'],
      ['url-1', 'url-2', 'url-3'],
    ))
    const dom = view.dom as HTMLElement
    const originalImages = Array.from(dom.querySelectorAll('img'))
    dom.classList.add('album-cycling')

    expect(view.update?.(albumNode(
      ['att-2', 'att-3', 'att-1'],
      ['url-2', 'url-3', 'url-1'],
      true,
    ), [], DecorationSet.empty)).toBe(true)

    const cycledImages = Array.from(dom.querySelectorAll('img'))
    expect(cycledImages).toEqual([originalImages[1], originalImages[2], originalImages[0]])
    expect(cycledImages.map(image => image.getAttribute('src'))).toEqual([
      'url-2',
      'url-3',
      'url-1',
    ])
    expect(dom.classList.contains('album-cycling')).toBe(true)
    expect(dom.dataset.hasCycled).toBe('true')
  })

  it('updates only the changed source on an existing image element', () => {
    const view = createAlbumNodeView(albumNode(['att-1'], ['old-url']))
    const dom = view.dom as HTMLElement
    const image = dom.querySelector('img')!
    image.dataset.imageLoadingInitialized = 'true'
    image.classList.remove('editor-image-loading')
    image.classList.add('editor-image-loaded')

    view.update?.(albumNode(['att-1'], ['new-url']), [], DecorationSet.empty)

    expect(dom.querySelector('img')).toBe(image)
    expect(image.getAttribute('src')).toBe('new-url')
    expect(image.classList.contains('editor-image-loading')).toBe(true)
    expect(image.classList.contains('editor-image-loaded')).toBe(false)
  })
})

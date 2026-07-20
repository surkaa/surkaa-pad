import { describe, expect, it } from 'vitest'
import {
  addImageToAlbumDocument,
  changeAlbumDisplayMode,
  createAlbumDocument,
  listAlbums,
  splitAlbumDocument,
  type EditorJsonNode,
} from '../albumEditor'

describe('createAlbumDocument', () => {
  it('inserts the album at the initial image position and removes selected images', () => {
    const document = {
      type: 'doc',
      content: [
        { type: 'paragraph' },
        { type: 'imageNode', attrs: { id: '2.jpg' } },
        { type: 'paragraph' },
        { type: 'imageNode', attrs: { id: '1.jpg' } },
        { type: 'imageNode', attrs: { id: '3.jpg' } },
      ],
    }

    expect(createAlbumDocument(
      document,
      ['1.jpg', '2.jpg'],
      '1.jpg',
      'album-1',
      'stackedCards',
      { '1.jpg': 'url-1', '2.jpg': 'url-2' },
    )).toEqual({
      type: 'doc',
      content: [
        { type: 'paragraph' },
        { type: 'paragraph' },
        {
          type: 'albumNode',
          attrs: {
            id: 'album-1',
            images: ['1.jpg', '2.jpg'],
            displayMode: 'stackedCards',
            urls: ['url-1', 'url-2'],
          },
        },
        { type: 'imageNode', attrs: { id: '3.jpg' } },
      ],
    })
  })
})

describe('changeAlbumDisplayMode', () => {
  it('changes only the target album', () => {
    const document = {
      type: 'doc',
      content: [
        { type: 'albumNode', attrs: { id: 'a1', displayMode: 'horizontalList' } },
        { type: 'albumNode', attrs: { id: 'a2', displayMode: 'horizontalList' } },
      ],
    }

    expect(changeAlbumDisplayMode(document, 'a1', 'stackedCards')).toEqual({
      type: 'doc',
      content: [
        { type: 'albumNode', attrs: { id: 'a1', displayMode: 'stackedCards' } },
        { type: 'albumNode', attrs: { id: 'a2', displayMode: 'horizontalList' } },
      ],
    })
  })
})

function albumDocument(images = ['a.jpg', 'b.jpg', 'c.jpg', 'd.jpg']): EditorJsonNode {
  return {
    type: 'doc',
    content: [
      { type: 'paragraph', content: [{ type: 'text', text: 'before' }] },
      {
        type: 'albumNode',
        attrs: {
          id: 'album-1',
          images,
          urls: images.map(image => `url-${image}`),
          displayMode: 'stackedCards',
          hasCycled: true,
        },
      },
      { type: 'paragraph', content: [{ type: 'text', text: 'after' }] },
    ],
  }
}

describe('splitAlbumDocument', () => {
  it('splits the entire album into ordered image nodes', () => {
    const result = splitAlbumDocument(albumDocument(), 'album-1', 'c.jpg', { type: 'all' })

    expect(result.content?.slice(1, 5)).toEqual([
      { type: 'imageNode', attrs: { id: 'a.jpg', src: 'url-a.jpg' } },
      { type: 'imageNode', attrs: { id: 'b.jpg', src: 'url-b.jpg' } },
      { type: 'imageNode', attrs: { id: 'c.jpg', src: 'url-c.jpg' } },
      { type: 'imageNode', attrs: { id: 'd.jpg', src: 'url-d.jpg' } },
    ])
  })

  it('moves only the selected image before the remaining album', () => {
    const result = splitAlbumDocument(albumDocument(), 'album-1', 'c.jpg', {
      type: 'single',
      position: 'before',
    })

    expect(result.content?.[1]).toEqual({
      type: 'imageNode',
      attrs: { id: 'c.jpg', src: 'url-c.jpg' },
    })
    expect(result.content?.[2].attrs).toMatchObject({
      id: 'album-1',
      images: ['a.jpg', 'b.jpg', 'd.jpg'],
      urls: ['url-a.jpg', 'url-b.jpg', 'url-d.jpg'],
      displayMode: 'stackedCards',
      hasCycled: true,
    })
  })

  it('moves only the selected image after the album', () => {
    const result = splitAlbumDocument(albumDocument(), 'album-1', 'b.jpg', {
      type: 'single',
      position: 'after',
    })

    expect(result.content?.[1].attrs?.images).toEqual(['a.jpg', 'c.jpg', 'd.jpg'])
    expect(result.content?.[2]).toEqual({
      type: 'imageNode',
      attrs: { id: 'b.jpg', src: 'url-b.jpg' },
    })
  })

  it('turns a one-image remainder into a normal image', () => {
    const result = splitAlbumDocument(
      albumDocument(['a.jpg', 'b.jpg']),
      'album-1',
      'a.jpg',
      { type: 'single', position: 'after' },
    )

    expect(result.content?.slice(1, 3)).toEqual([
      { type: 'imageNode', attrs: { id: 'b.jpg', src: 'url-b.jpg' } },
      { type: 'imageNode', attrs: { id: 'a.jpg', src: 'url-a.jpg' } },
    ])
  })

  it('splits the selected image and every image before it', () => {
    const result = splitAlbumDocument(albumDocument(), 'album-1', 'b.jpg', {
      type: 'range',
      direction: 'before',
    })

    expect(result.content?.slice(1, 4)).toEqual([
      { type: 'imageNode', attrs: { id: 'a.jpg', src: 'url-a.jpg' } },
      { type: 'imageNode', attrs: { id: 'b.jpg', src: 'url-b.jpg' } },
      expect.objectContaining({
        type: 'albumNode',
        attrs: expect.objectContaining({ images: ['c.jpg', 'd.jpg'] }),
      }),
    ])
  })

  it('splits the selected image and every image after it', () => {
    const result = splitAlbumDocument(albumDocument(), 'album-1', 'c.jpg', {
      type: 'range',
      direction: 'after',
    })

    expect(result.content?.slice(1, 4)).toEqual([
      expect.objectContaining({
        type: 'albumNode',
        attrs: expect.objectContaining({ images: ['a.jpg', 'b.jpg'] }),
      }),
      { type: 'imageNode', attrs: { id: 'c.jpg', src: 'url-c.jpg' } },
      { type: 'imageNode', attrs: { id: 'd.jpg', src: 'url-d.jpg' } },
    ])
  })

  it('does not modify the document when the album or selected image is missing', () => {
    const document = albumDocument()

    expect(splitAlbumDocument(document, 'missing', 'a.jpg', { type: 'all' })).toBe(document)
    expect(splitAlbumDocument(document, 'album-1', 'missing.jpg', {
      type: 'single',
      position: 'before',
    })).toBe(document)
  })
})

describe('addImageToAlbumDocument', () => {
  it('removes a normal image and inserts it into the selected album position', () => {
    const document = {
      type: 'doc',
      content: [
        { type: 'imageNode', attrs: { id: 'single.jpg', src: 'old-url' } },
        ...(albumDocument(['a.jpg', 'b.jpg']).content || []),
      ],
    }

    const result = addImageToAlbumDocument(
      document,
      'single.jpg',
      'album-1',
      1,
      { 'single.jpg': 'current-url' },
    )

    expect(result.content?.some(node => node.type === 'imageNode')).toBe(false)
    expect(result.content?.[1].attrs).toMatchObject({
      images: ['a.jpg', 'single.jpg', 'b.jpg'],
      urls: ['url-a.jpg', 'current-url', 'url-b.jpg'],
    })
  })

  it('supports inserting after the last image when the source follows the album', () => {
    const document = {
      type: 'doc',
      content: [
        ...(albumDocument(['a.jpg', 'b.jpg']).content || []),
        { type: 'imageNode', attrs: { id: 'single.jpg', src: 'source-url' } },
      ],
    }

    const result = addImageToAlbumDocument(
      document,
      'single.jpg',
      'album-1',
      2,
      {},
    )

    expect(result.content?.[1].attrs).toMatchObject({
      images: ['a.jpg', 'b.jpg', 'single.jpg'],
      urls: ['url-a.jpg', 'url-b.jpg', 'source-url'],
    })
  })

  it.each([-1, 3, 0.5])('rejects invalid insertion index %s', insertionIndex => {
    const document = {
      type: 'doc',
      content: [
        { type: 'imageNode', attrs: { id: 'single.jpg' } },
        ...(albumDocument(['a.jpg', 'b.jpg']).content || []),
      ],
    }

    expect(addImageToAlbumDocument(
      document,
      'single.jpg',
      'album-1',
      insertionIndex,
      {},
    )).toBe(document)
  })

  it('rejects duplicate images and missing sources', () => {
    const document = albumDocument(['a.jpg', 'b.jpg'])
    document.content!.push({ type: 'imageNode', attrs: { id: 'a.jpg' } })

    expect(addImageToAlbumDocument(document, 'a.jpg', 'album-1', 1, {})).toBe(document)
    expect(addImageToAlbumDocument(document, 'missing.jpg', 'album-1', 1, {})).toBe(document)
  })
})

describe('listAlbums', () => {
  it('returns valid albums in document order', () => {
    const document = albumDocument(['a.jpg', 'b.jpg'])
    document.content!.push({
      type: 'albumNode',
      attrs: { id: 'album-2', images: ['c.jpg'], urls: ['url-c.jpg'] },
    })

    expect(listAlbums(document)).toEqual([
      {
        id: 'album-1',
        images: ['a.jpg', 'b.jpg'],
        urls: ['url-a.jpg', 'url-b.jpg'],
        displayMode: 'stackedCards',
      },
      {
        id: 'album-2',
        images: ['c.jpg'],
        urls: ['url-c.jpg'],
        displayMode: 'horizontalList',
      },
    ])
  })
})

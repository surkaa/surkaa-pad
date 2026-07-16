import { describe, expect, it } from 'vitest'
import { changeAlbumDisplayMode, createAlbumDocument } from '../albumEditor'

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

import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    albumNode: {
      insertAlbum: (attrs: {
        id: string
        images: string[]
        displayMode: 'horizontalList' | 'stackedCards'
        urls: string[]
      }) => ReturnType
    }
  }
}

export const AlbumNode = Node.create({
  name: 'albumNode',

  group: 'block',
  selectable: true,
  draggable: true,
  atom: true,

  addAttributes() {
    return {
      id: { default: null },
      images: { default: [] },
      displayMode: { default: 'horizontalList' },
      urls: { default: [] },
    }
  },

  parseHTML() {
    return [{
      tag: 'div.editor-image-album',
      getAttrs: el => {
        const element = el as HTMLElement
        return {
          id: element.dataset.id,
          images: parseJsonArray(element.dataset.images),
          displayMode: element.dataset.displayMode || 'horizontalList',
          urls: parseJsonArray(element.dataset.urls),
        }
      },
    }]
  },

  renderHTML({ node }) {
    const images = node.attrs.images as string[]
    const urls = node.attrs.urls as string[]
    const children = images.map((filename, index) => [
      'img',
      {
        src: urls[index] || '',
        'data-id': filename,
        loading: 'lazy',
      },
    ])

    return [
      'div',
      mergeAttributes({
        class: 'editor-image-album',
        'data-id': node.attrs.id,
        'data-images': JSON.stringify(images),
        'data-display-mode': node.attrs.displayMode,
        'data-urls': JSON.stringify(urls),
      }),
      ...children,
    ]
  },

  addCommands() {
    return {
      insertAlbum: attrs => ({ commands }) =>
        commands.insertContent({ type: this.name, attrs }),
    }
  },
})

function parseJsonArray(value?: string): string[] {
  if (!value) return []
  try {
    const parsed = JSON.parse(value)
    return Array.isArray(parsed) ? parsed.filter(item => typeof item === 'string') : []
  } catch {
    return []
  }
}

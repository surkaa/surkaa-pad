import { Node, mergeAttributes } from '@tiptap/vue-3'
import type { Node as ProseMirrorNode } from '@tiptap/pm/model'
import type { NodeView } from '@tiptap/pm/view'

const IMAGE_LOADING_INITIALIZED = 'imageLoadingInitialized'

function syncAlbumDom(dom: HTMLElement, node: ProseMirrorNode) {
  const images = node.attrs.images as string[]
  const urls = node.attrs.urls as string[]
  dom.classList.add('editor-image-album')
  dom.dataset.id = node.attrs.id || ''
  dom.dataset.images = JSON.stringify(images)
  dom.dataset.displayMode = node.attrs.displayMode
  dom.dataset.urls = JSON.stringify(urls)
  dom.dataset.hasCycled = String(node.attrs.hasCycled)

  const existingImages = new Map(
    Array.from(dom.querySelectorAll<HTMLImageElement>(':scope > img[data-id]'))
      .map(image => [image.dataset.id!, image]),
  )

  images.forEach((attachmentId, index) => {
    let image = existingImages.get(attachmentId)
    if (image) {
      existingImages.delete(attachmentId)
    } else {
      image = document.createElement('img')
      image.dataset.id = attachmentId
      image.classList.add('editor-image-loading')
      image.loading = 'lazy'
    }

    const url = urls[index] || ''
    if (image.getAttribute('src') !== url) {
      if (image.dataset[IMAGE_LOADING_INITIALIZED] === 'true') {
        image.classList.remove('editor-image-loaded', 'editor-image-error')
        image.classList.add('editor-image-loading')
      }
      image.setAttribute('src', url)
    }
    dom.appendChild(image)
  })

  existingImages.forEach(image => image.remove())
}

export function createAlbumNodeView(initialNode: ProseMirrorNode): NodeView {
  let currentNode = initialNode
  const dom = document.createElement('div')
  syncAlbumDom(dom, currentNode)

  return {
    dom,
    update(node) {
      if (node.type !== currentNode.type) return false
      currentNode = node
      syncAlbumDom(dom, currentNode)
      return true
    },
    ignoreMutation: () => true,
  }
}

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
      hasCycled: { default: false },
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
          hasCycled: element.dataset.hasCycled === 'true',
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
        class: 'editor-image-loading',
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
        'data-has-cycled': String(node.attrs.hasCycled),
      }),
      ...children,
    ]
  },

  addNodeView() {
    return ({ node }) => createAlbumNodeView(node)
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

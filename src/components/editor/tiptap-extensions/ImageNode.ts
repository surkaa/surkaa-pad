import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    imageNode: {
      insertImage: (attrs: { id: string; size?: string; src?: string }) => ReturnType
    }
  }
}

export const ImageNode = Node.create({
  name: 'imageNode',

  group: 'block',
  selectable: true,
  draggable: true,
  atom: true,

  addAttributes() {
    return {
      id: { default: null },
      src: { default: null },
      size: { default: null },
    }
  },

  parseHTML() {
    return [
      {
        tag: 'img[data-id]',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
          src: (el as HTMLImageElement).getAttribute('src'),
          size: (el as HTMLElement).getAttribute('data-size'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    const attrs: Record<string, string> = {
      src: node.attrs.src || '',
      'data-id': node.attrs.id,
      loading: 'lazy',
    }
    if (node.attrs.size === 'small') {
      attrs['data-size'] = 'small'
    }
    return ['img', mergeAttributes(attrs)]
  },

  addCommands() {
    return {
      insertImage:
        (attrs: { id: string; size?: string; src?: string }) =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name, attrs })
        },
    }
  },
})

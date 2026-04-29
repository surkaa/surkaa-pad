import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    videoNode: {
      insertVideo: (attrs: { id: string; src?: string }) => ReturnType
    }
  }
}

export const VideoNode = Node.create({
  name: 'videoNode',

  group: 'block',
  selectable: true,
  draggable: true,
  atom: true,

  addAttributes() {
    return {
      id: { default: null },
      src: { default: null },
    }
  },

  parseHTML() {
    return [
      {
        tag: 'video[data-id]',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
          src: (el as HTMLElement).getAttribute('src'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    return [
      'video',
      mergeAttributes({
        src: node.attrs.src || '',
        'data-id': node.attrs.id,
        controls: 'true',
      }),
    ]
  },

  addCommands() {
    return {
      insertVideo:
        (attrs: { id: string; src?: string }) =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name, attrs })
        },
    }
  },
})

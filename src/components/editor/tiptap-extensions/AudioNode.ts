import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    audioNode: {
      insertAudio: (attrs: { id: string; src?: string }) => ReturnType
    }
  }
}

export const AudioNode = Node.create({
  name: 'audioNode',

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
        tag: 'audio[data-id]',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
          src: (el as HTMLElement).getAttribute('src'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    return [
      'audio',
      mergeAttributes({
        src: node.attrs.src || '',
        'data-id': node.attrs.id,
        controls: 'true',
      }),
    ]
  },

  addCommands() {
    return {
      insertAudio:
        (attrs: { id: string; src?: string }) =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name, attrs })
        },
    }
  },
})

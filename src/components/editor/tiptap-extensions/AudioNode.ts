import { Node, mergeAttributes } from '@tiptap/core'

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    audioNode: {
      insertAudio: (attrs: { id: string }) => ReturnType
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
    }
  },

  parseHTML() {
    return [
      {
        tag: 'audio[data-id]',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    const storage = this.editor.storage.attachmentStorage as {
      attachmentMap: Record<string, string>
    } | undefined
    return [
      'audio',
      mergeAttributes({
        src: storage?.attachmentMap[node.attrs.id] || '',
        'data-id': node.attrs.id,
        controls: 'true',
      }),
    ]
  },

  addCommands() {
    return {
      insertAudio:
        (attrs: { id: string }) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs,
          })
        },
    }
  },
})

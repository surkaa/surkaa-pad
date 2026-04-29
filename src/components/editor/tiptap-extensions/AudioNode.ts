import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
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
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const storage = (this.editor!.storage as Record<string, any>).attachmentStorage as
      { attachmentMap: Record<string, string> } | undefined
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

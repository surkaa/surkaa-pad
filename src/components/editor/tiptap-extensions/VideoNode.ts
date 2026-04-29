import { Node, mergeAttributes } from '@tiptap/core'

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    videoNode: {
      insertVideo: (attrs: { id: string }) => ReturnType
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
    }
  },

  parseHTML() {
    return [
      {
        tag: 'video[data-id]',
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
      'video',
      mergeAttributes({
        src: storage?.attachmentMap[node.attrs.id] || '',
        'data-id': node.attrs.id,
        controls: 'true',
      }),
    ]
  },

  addCommands() {
    return {
      insertVideo:
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

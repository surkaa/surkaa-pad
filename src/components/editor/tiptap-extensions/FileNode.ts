import { Node, mergeAttributes } from '@tiptap/vue-3'

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    fileNode: {
      insertFile: (attrs: { id: string }) => ReturnType
    }
  }
}

export const FileNode = Node.create({
  name: 'fileNode',

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
        tag: 'div.editor-file-attachment',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const storage = (this.editor!.storage as Record<string, any>).attachmentStorage as {
      attachmentMap: Record<string, string>
      getAttachment?: (filename: string) => { size: number } | null
    } | undefined
    const att = storage?.getAttachment?.(node.attrs.id)
    const sizeText = att ? formatBytes(att.size) : ''
    return [
      'div',
      mergeAttributes({
        'data-id': node.attrs.id,
        class: 'editor-file-attachment',
        contenteditable: 'false',
      }),
      ['div', { class: 'file-title' },
        ['span', { class: 'file-icon' }, '📎'],
        ['span', { class: 'file-name' }, node.attrs.id],
      ],
      ['span', { class: 'file-size' }, sizeText],
    ]
  },

  addCommands() {
    return {
      insertFile:
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

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

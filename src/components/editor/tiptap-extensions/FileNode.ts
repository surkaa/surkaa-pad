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
      ['span', { class: 'file-size' }, ''],
    ]
  },

  addCommands() {
    return {
      insertFile:
        (attrs: { id: string }) =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name, attrs })
        },
    }
  },
})

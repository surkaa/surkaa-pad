import { Node, mergeAttributes } from '@tiptap/core'

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    imageNode: {
      insertImage: (attrs: { id: string; size?: string }) => ReturnType
    }
  }
}

export interface ImageNodeOptions {
  defaultImageSizeIsSmall: boolean | (() => boolean)
}

export const ImageNode = Node.create<ImageNodeOptions>({
  name: 'imageNode',

  group: 'block',
  selectable: true,
  draggable: true,
  atom: true,

  addOptions() {
    return {
      defaultImageSizeIsSmall: false,
    }
  },

  addAttributes() {
    return {
      id: { default: null },
      size: { default: null },
    }
  },

  parseHTML() {
    return [
      {
        tag: 'img[data-id]',
        getAttrs: (el) => ({
          id: (el as HTMLElement).getAttribute('data-id'),
          size: (el as HTMLElement).getAttribute('data-size'),
        }),
      },
    ]
  },

  renderHTML({ node }) {
    const storage = this.editor.storage.attachmentStorage as {
      attachmentMap: Record<string, string>
    } | undefined
    const url = storage?.attachmentMap[node.attrs.id] || ''
    const attrs: Record<string, string> = {
      src: url,
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
        (attrs: { id: string; size?: string }) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs,
          })
        },
    }
  },
})

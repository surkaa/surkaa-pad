import type { Editor } from '@tiptap/vue-3'
import { TextSelection } from '@tiptap/pm/state'

export interface InsertedImageAttributes {
  id: string
  size?: string
  src?: string
}

/**
 * 插入拍摄的图片后，将选区移动到图片后的文本块。
 *
 * 图片是可选中的原子块；若选区继续停留在刚插入的图片上，下一次拍摄插入
 * 会替换该图片。这里同时保证图片后存在段落，使后续插入只会追加。
 */
export function insertImageAndMoveCursorAfter(
  editor: Editor,
  attrs: InsertedImageAttributes,
): boolean {
  if (!editor.commands.insertImage(attrs)) return false

  let imagePosition: number | null = null
  let imageNodeSize = 0
  editor.state.doc.descendants((node, position) => {
    if (node.type.name === 'imageNode' && node.attrs.id === attrs.id) {
      imagePosition = position
      imageNodeSize = node.nodeSize
      return false
    }
    return true
  })
  if (imagePosition === null) return true

  const afterImage = imagePosition + imageNodeSize
  const transaction = editor.state.tr
  if (!transaction.doc.nodeAt(afterImage)?.isTextblock) {
    const paragraph = editor.schema.nodes.paragraph
    if (!paragraph) return true
    transaction.insert(afterImage, paragraph.create())
  }
  transaction.setSelection(TextSelection.create(transaction.doc, afterImage + 1))
  editor.view.dispatch(transaction)
  return true
}

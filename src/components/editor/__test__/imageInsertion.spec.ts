// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import { NodeSelection, TextSelection } from '@tiptap/pm/state'
import { ImageNode } from '../tiptap-extensions/ImageNode'
import { insertImageAndMoveCursorAfter } from '../imageInsertion'

describe('camera image insertion', () => {
  it('moves the selection off each inserted image so consecutive photos append', () => {
    const editor = new Editor({
      extensions: [StarterKit, ImageNode],
      content: '<p>拍摄记录</p>',
    })
    editor.commands.setTextSelection(editor.state.doc.content.size)

    expect(insertImageAndMoveCursorAfter(editor, {id: 'photo-1', src: 'url://1'})).toBe(true)
    expect(editor.state.selection).toBeInstanceOf(TextSelection)
    expect(editor.state.selection).not.toBeInstanceOf(NodeSelection)
    expect(editor.state.selection.$from.parent.isTextblock).toBe(true)

    expect(insertImageAndMoveCursorAfter(editor, {id: 'photo-2', src: 'url://2'})).toBe(true)
    expect(editor.getJSON().content?.filter(node => node.type === 'imageNode')).toEqual([
      expect.objectContaining({attrs: expect.objectContaining({id: 'photo-1'})}),
      expect.objectContaining({attrs: expect.objectContaining({id: 'photo-2'})}),
    ])
    expect(editor.state.selection).toBeInstanceOf(TextSelection)
    expect(editor.state.selection.$from.parent.isTextblock).toBe(true)
    editor.destroy()
  })
})

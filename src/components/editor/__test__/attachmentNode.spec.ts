// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { findAttachmentNode } from '../attachmentNode'

describe('findAttachmentNode', () => {
  it('finds supported attachment nodes through nested click targets', () => {
    const wrapper = document.createElement('div')
    const file = document.createElement('div')
    file.className = 'editor-file-attachment'
    file.dataset.id = 'att-file'
    const label = document.createElement('span')
    file.append(label)
    wrapper.append(file)

    expect(findAttachmentNode(label, wrapper)).toMatchObject({
      type: 'file',
      attachmentId: 'att-file',
      el: file,
    })
  })

  it('recognizes image, video and audio nodes', () => {
    for (const [tag, type] of [
      ['img', 'image'],
      ['video', 'video'],
      ['audio', 'audio'],
    ] as const) {
      const element = document.createElement(tag)
      element.dataset.id = `att-${type}`
      expect(findAttachmentNode(element)).toMatchObject({
        type,
        attachmentId: `att-${type}`,
      })
    }
  })

  it('stops at the editor boundary and ignores ordinary elements', () => {
    const wrapper = document.createElement('div')
    wrapper.dataset.id = 'outside'
    const paragraph = document.createElement('p')
    wrapper.append(paragraph)

    expect(findAttachmentNode(paragraph, wrapper)).toBeNull()
    expect(findAttachmentNode(null, wrapper)).toBeNull()
  })
})

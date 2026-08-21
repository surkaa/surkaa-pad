/**
 * Tiptap HTML ↔ 结构化日记内容双向转换。
 * Markdown 只负责文本节点；附件节点始终直接转换，不经过文本标记。
 */
import type {DiaryContent, DiaryContentNode} from '../../bindings'

const INLINE_TAGS: Record<string, [string, string]> = {
  STRONG: ['**', '**'],
  B: ['**', '**'],
  EM: ['*', '*'],
  I: ['*', '*'],
  S: ['~~', '~~'],
  DEL: ['~~', '~~'],
  CODE: ['`', '`'],
  U: ['<u>', '</u>'],
}

export function htmlToMarkdown(html: string): string {
  if (!html) return ''

  const doc = new DOMParser().parseFromString(html, 'text/html')
  const parts: string[] = []
  walkBlocks(doc.body, parts)
  return parts.join('\n\n').replace(/\n{3,}/g, '\n\n').trim()
}

export function diaryContentToHtml(
  content: DiaryContent,
  attachmentMap: Record<string, string>,
  attachmentFilenames: Record<string, string> = {},
): string {
  const html = content.nodes.map(node => {
    switch (node.type) {
      case 'markdown':
        return markdownToHtml(node.text)
      case 'summary':
        return renderSummary(node.summary, node.content)
      case 'image':
        return renderImage(node.attachmentId, attachmentMap[node.attachmentId] || '', node.size)
      case 'video':
        return `<video controls src="${escapeAttr(attachmentMap[node.attachmentId] || '')}" data-id="${escapeAttr(node.attachmentId)}"></video>`
      case 'audio':
        return `<audio controls src="${escapeAttr(attachmentMap[node.attachmentId] || '')}" data-id="${escapeAttr(node.attachmentId)}"></audio>`
      case 'file':
        return `<div data-id="${escapeAttr(node.attachmentId)}" data-filename="${escapeAttr(attachmentFilenames[node.attachmentId] || node.attachmentId)}" class="editor-file-attachment"></div>`
      case 'album':
        return renderAlbum(node.id, node.attachmentIds, node.displayMode, attachmentMap)
    }
  }).join('')
  return html || '<p></p>'
}

export function diaryContentToSource(content: DiaryContent): string {
  return JSON.stringify(content, null, 2)
}

export function htmlToDiaryContent(html: string): DiaryContent {
  if (!html) return {nodes: []}

  const doc = new DOMParser().parseFromString(html, 'text/html')
  const nodes: DiaryContentNode[] = []
  let markdownBlocks: string[] = []

  const flushMarkdown = () => {
    if (!markdownBlocks.length) return
    const text = markdownBlocks.join('\n\n').replace(/\n{3,}/g, '\n\n')
    if (text) nodes.push({type: 'markdown', text})
    markdownBlocks = []
  }

  for (const child of doc.body.childNodes) {
    if (child.nodeType === Node.ELEMENT_NODE) {
      const structuredNode = elementToStructuredNode(child as HTMLElement)
      if (structuredNode) {
        flushMarkdown()
        nodes.push(structuredNode)
        continue
      }
    }

    const block = nodeToMarkdown(child)
    if (block.trim()) markdownBlocks.push(block)
  }

  flushMarkdown()
  return {nodes}
}

function elementToStructuredNode(element: HTMLElement): DiaryContentNode | null {
  const tag = element.tagName.toUpperCase()
  const attachmentId = element.dataset.id

  if (tag === 'DETAILS' && element.classList.contains('editor-summary')) {
    return {
      type: 'summary',
      summary: element.dataset.summary
        ?? element.querySelector('summary')?.textContent
        ?? '',
      content: element.dataset.content
        ?? element.querySelector('.editor-summary-content')?.textContent
        ?? '',
    }
  }

  if (element.classList.contains('editor-image-album') && attachmentId) {
    return {
      type: 'album',
      id: attachmentId,
      attachmentIds: parseJsonArray(element.dataset.images),
      displayMode: element.dataset.displayMode === 'stackedCards'
        ? 'stackedCards'
        : 'horizontalList',
    }
  }
  if (element.classList.contains('editor-file-attachment') && attachmentId) {
    return {type: 'file', attachmentId}
  }
  if (tag === 'IMG' && attachmentId) {
    return {
      type: 'image',
      attachmentId,
      size: element.dataset.size === 'small' ? 'small' : 'normal',
    }
  }
  if (tag === 'VIDEO' && attachmentId) return {type: 'video', attachmentId}
  if (tag === 'AUDIO' && attachmentId) return {type: 'audio', attachmentId}
  return null
}

function nodeToMarkdown(node: Node): string {
  const container = node.ownerDocument?.createElement('div') ?? document.createElement('div')
  container.appendChild(node.cloneNode(true))
  const parts: string[] = []
  walkBlocks(container, parts)
  return parts.join('\n\n').replace(/\n{3,}/g, '\n\n').trim()
}

function renderImage(
  attachmentId: string,
  url: string,
  size: 'normal' | 'small',
): string {
  const sizeAttr = size === 'small' ? ' data-size="small"' : ''
  return `<img src="${escapeAttr(url)}" data-id="${escapeAttr(attachmentId)}"${sizeAttr}>`
}

function renderSummary(summary: string, content: string): string {
  return `<details class="editor-summary" data-summary="${escapeAttr(summary)}" data-content="${escapeAttr(content)}">`
    + `<summary>${escapeHtml(summary)}</summary>`
    + `<div class="editor-summary-content">${escapeHtml(content)}</div>`
    + '</details>'
}

function renderAlbum(
  id: string,
  attachmentIds: string[],
  displayMode: 'horizontalList' | 'stackedCards',
  attachmentMap: Record<string, string>,
): string {
  const urls = attachmentIds.map(attachmentId => attachmentMap[attachmentId] || '')
  return `<div class="editor-image-album" data-id="${escapeAttr(id)}" data-images="${escapeAttr(JSON.stringify(attachmentIds))}" data-display-mode="${displayMode}" data-urls="${escapeAttr(JSON.stringify(urls))}">`
    + attachmentIds.map((attachmentId, index) =>
      `<img src="${escapeAttr(urls[index])}" data-id="${escapeAttr(attachmentId)}">`
    ).join('')
    + '</div>'
}

function walkBlocks(parent: Node, out: string[]): void {
  for (const child of parent.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      const text = child.textContent || ''
      if (text.trim()) out.push(text.trim())
      continue
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue

    const element = child as HTMLElement
    const tag = element.tagName.toUpperCase()

    // 结构化节点由 htmlToDiaryContent 直接读取，不属于 Markdown 文本。
    if (elementToStructuredNode(element)) continue

    switch (tag) {
      case 'BR':
        break
      case 'P':
        out.push(serializeInline(element))
        break
      case 'H1':
      case 'H2':
      case 'H3':
      case 'H4':
      case 'H5':
      case 'H6': {
        const level = Number.parseInt(tag[1], 10)
        out.push(`${'#'.repeat(level)} ${serializeInline(element)}`)
        break
      }
      case 'UL':
      case 'OL': {
        if (tag === 'UL' && isTaskList(element)) {
          const items: string[] = []
          for (const listItem of element.children) {
            if (!isTaskItem(listItem)) continue
            const checked = listItem.getAttribute('data-checked') === 'true'
              || (listItem.querySelector('input[type="checkbox"]') as HTMLInputElement | null)?.checked
            items.push(`- [${checked ? 'x' : ' '}] ${serializeTaskItem(listItem)}`.trimEnd())
          }
          if (items.length) out.push(items.join('\n'))
          break
        }
        const items: string[] = []
        for (const listItem of element.children) {
          if (listItem.tagName.toUpperCase() !== 'LI') continue
          const prefix = tag === 'OL' ? `${items.length + 1}. ` : '- '
          items.push(prefix + serializeInline(listItem))
        }
        if (items.length) out.push(items.join('\n'))
        break
      }
      case 'LI':
        out.push(`- ${serializeInline(element)}`)
        break
      case 'BLOCKQUOTE': {
        const text = serializeInline(element)
        out.push(text.split('\n').map(line => `> ${line}`).join('\n'))
        break
      }
      case 'HR':
        out.push('---')
        break
      case 'PRE': {
        const code = element.querySelector('code')
        out.push(`\`\`\`\n${(code || element).textContent || ''}\n\`\`\``)
        break
      }
      case 'DIV':
        if (element.children.length > 0) {
          walkBlocks(element, out)
        } else {
          const text = serializeInline(element)
          if (text.trim()) out.push(text.trim())
        }
        break
      default: {
        const text = serializeInline(element)
        if (text.trim()) out.push(text.trim())
      }
    }
  }
}

function serializeInline(parent: Node): string {
  let result = ''
  for (const child of parent.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += child.textContent || ''
      continue
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue

    const element = child as HTMLElement
    const tag = element.tagName.toUpperCase()
    if (tag === 'BR') {
      result += '\n'
      continue
    }

    const marker = INLINE_TAGS[tag]
    if (marker) {
      result += marker[0] + serializeInline(element) + marker[1]
    } else if (tag === 'A') {
      result += `[${serializeInline(element)}](${element.getAttribute('href') || ''})`
    } else {
      result += serializeInline(element)
    }
  }
  return result
}

export function markdownToHtml(markdown: string): string {
  if (!markdown) return '<p></p>'
  if (markdown.startsWith('<')) return markdown

  const blocks = markdown.split(/\n{2,}/)
  const htmlBlocks: string[] = []

  for (const block of blocks) {
    if (!block.trim()) continue
    const trimmed = block.trim()

    if (/^```[\s\S]*```$/.test(trimmed)) {
      const code = trimmed.replace(/^```\n?/, '').replace(/\n?```$/, '')
      htmlBlocks.push(`<pre><code>${escapeHtml(code)}</code></pre>`)
      continue
    }

    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/m)
    if (headingMatch) {
      const level = headingMatch[1].length
      htmlBlocks.push(`<h${level}>${processInline(headingMatch[2])}</h${level}>`)
      continue
    }

    if (/^---$/.test(trimmed) || /^\*\*\*$/.test(trimmed)) {
      htmlBlocks.push('<hr>')
      continue
    }

    if (trimmed.startsWith('> ')) {
      const lines = trimmed.split('\n').map(line => line.replace(/^> /, '')).join('\n')
      htmlBlocks.push(`<blockquote><p>${processInline(lines)}</p></blockquote>`)
      continue
    }

    const taskItems = parseTaskListItems(trimmed)
    if (taskItems) {
      htmlBlocks.push('<ul data-type="taskList">' + taskItems.map(item =>
        `<li data-type="taskItem" data-checked="${item.checked}"><p>${processInline(item.text)}</p></li>`
      ).join('') + '</ul>')
      continue
    }

    if (/^- .+/.test(trimmed)) {
      const items = trimmed.split('\n').filter(line => line.startsWith('- '))
      htmlBlocks.push('<ul>' + items.map(item =>
        `<li><p>${processInline(item.slice(2))}</p></li>`
      ).join('') + '</ul>')
      continue
    }

    if (/^\d+\. .+/.test(trimmed)) {
      const items = trimmed.split('\n').filter(line => /^\d+\. .+/.test(line))
      htmlBlocks.push('<ol>' + items.map(item =>
        `<li><p>${processInline(item.replace(/^\d+\. /, ''))}</p></li>`
      ).join('') + '</ol>')
      continue
    }

    htmlBlocks.push(`<p>${trimmed.split('\n').map(processInline).join('<br>')}</p>`)
  }

  return htmlBlocks.join('') || '<p></p>'
}

function isTaskList(element: HTMLElement): boolean {
  return element.dataset.type === 'taskList'
    || Array.from(element.children).some(isTaskItem)
}

function isTaskItem(element: Element): element is HTMLElement {
  return element.tagName.toUpperCase() === 'LI'
    && (element as HTMLElement).dataset.type === 'taskItem'
}

function serializeTaskItem(element: HTMLElement): string {
  const content = Array.from(element.children).find(child => child.tagName.toUpperCase() === 'DIV')
  return serializeInline(content || element).trim()
}

function parseTaskListItems(markdown: string): Array<{checked: boolean; text: string}> | null {
  const lines = markdown.split('\n')
  const items = lines.map(line => line.match(/^- \[([ xX])\](?:\s+(.*))?$/))
  if (items.some(item => !item)) return null
  return items.map(item => ({
    checked: item![1].toLowerCase() === 'x',
    text: item![2] || '',
  }))
}

function processInline(text: string): string {
  return text
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '<em>$1</em>')
    .replace(/~~([^~]+)~~/g, '<s>$1</s>')
    .replace(/<u>([^<]*)<\/u>/g, '<u>$1</u>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>')
}

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function parseJsonArray(value?: string): string[] {
  if (!value) return []
  try {
    const parsed = JSON.parse(value)
    return Array.isArray(parsed) ? parsed.filter(item => typeof item === 'string') : []
  } catch {
    return []
  }
}

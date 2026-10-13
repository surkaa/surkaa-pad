/**
 * Tiptap HTML ↔ 结构化日记内容双向转换。
 * Markdown 只负责文本节点；附件节点始终直接转换，不经过文本标记。
 */
import type {DiaryContent, DiaryContentNode, DiaryLocation} from '../../bindings'
import {renderEditorMarkdown} from './markdownParser'

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
  return parts.filter(part => part.trim()).join('\n\n').trim()
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
      case 'location':
        return renderLocation(node.location)
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
    const text = markdownBlocks.join('\n\n')
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

  if (element.classList.contains('editor-location')) {
    const location = parseLocation(element.dataset.location)
    return location ? {type: 'location', location} : null
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
  return parts.filter(part => part.trim()).join('\n\n').trim()
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

function renderLocation(location: DiaryLocation): string {
  const placeName = location.placeName?.trim() || '未命名地点'
  const accuracy = location.horizontalAccuracyMeters === null
    ? '精度未知'
    : `精度约 ±${Math.round(location.horizontalAccuracyMeters)} 米`
  return `<div class="editor-location" data-location="${escapeAttr(JSON.stringify(location))}">`
    + '<span class="editor-location-icon">📍</span>'
    + '<div class="editor-location-body">'
    + `<div class="editor-location-name">${escapeHtml(placeName)}</div>`
    + `<div class="editor-location-details">${location.latitude.toFixed(6)}, ${location.longitude.toFixed(6)} · ${accuracy}</div>`
    + '</div><button type="button" class="editor-location-open">↗</button></div>'
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
        out.push(serializeList(element))
        break
      }
      case 'LI':
        out.push(`- ${serializeInline(element)}`)
        break
      case 'TABLE':
        out.push(serializeTable(element as HTMLTableElement))
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
        const text = (code || element).textContent || ''
        const language = Array.from(code?.classList || []).find(name => name.startsWith('language-'))?.slice(9) || ''
        const longestRun = (text.match(/`+/g) || []).reduce((maximum, run) => Math.max(maximum, run.length), 0)
        const fence = '`'.repeat(Math.max(3, longestRun + 1))
        out.push(`${fence}${language}\n${text}\n${fence}`)
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

function serializeInline(parent: Node, tableCell = false): string {
  let result = ''
  for (const child of parent.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += tableCell ? escapeTableText(child.textContent || '') : child.textContent || ''
      continue
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue

    const element = child as HTMLElement
    const tag = element.tagName.toUpperCase()
    if (tag === 'BR') {
      result += tableCell ? '<br>' : '\n'
      continue
    }

    if (tag === 'CODE') {
      const text = element.textContent || ''
      if (tableCell && /\\+\|/.test(text)) {
        // Markdown 表格先分列再解析行内代码；这种组合改用 HTML code 保留反斜杠。
        result += `<code>${escapeTableText(text)}</code>`
      } else {
        const longestRun = (text.match(/`+/g) || []).reduce((maximum, run) => Math.max(maximum, run.length), 0)
        const fence = '`'.repeat(longestRun + 1)
        const padding = /^[` ]|[` ]$/.test(text) ? ' ' : ''
        result += fence + padding + (tableCell ? text.replace(/\|/g, '\\|') : text) + padding + fence
      }
      continue
    }

    const marker = INLINE_TAGS[tag]
    if (marker) {
      result += marker[0] + serializeInline(element, tableCell) + marker[1]
    } else if (tag === 'A') {
      const href = element.getAttribute('href') || ''
      result += `[${serializeInline(element, tableCell)}](${tableCell ? href.replace(/\|/g, '%7C') : href})`
    } else {
      result += serializeInline(element, tableCell)
    }
  }
  return result
}

function escapeTableText(text: string): string {
  return escapeHtml(text.replace(/\\/g, '\\\\').replace(/([|`*_\[\]~])/g, '\\$1'))
}

function serializeTable(table: HTMLTableElement): string {
  const rows = Array.from(table.rows)
  const grid: string[][] = []
  const alignments: string[] = []
  rows.forEach((row, rowIndex) => {
    grid[rowIndex] ||= []
    let column = 0
    for (const cell of Array.from(row.cells)) {
      while (grid[rowIndex][column] !== undefined) column++
      const blocks = Array.from(cell.childNodes).map(child => {
        if (child instanceof HTMLElement && child.tagName === 'P') return serializeInline(child, true)
        const wrapper = document.createElement('div')
        wrapper.append(child.cloneNode(true))
        return serializeInline(wrapper, true)
      }).filter(Boolean)
      const text = blocks.join('<br>').replace(/\r?\n/g, '<br>')
      const alignment = cell.style.textAlign || cell.getAttribute('align') || ''
      const colspan = Math.max(1, cell.colSpan)
      const rowspan = Math.min(Math.max(1, cell.rowSpan), rows.length - rowIndex)
      // GFM 没有合并单元格；展开占用位置，保留每个原始单元格的文字。
      for (let down = 0; down < rowspan; down++) {
        grid[rowIndex + down] ||= []
        for (let across = 0; across < colspan; across++) {
          grid[rowIndex + down][column + across] = down === 0 && across === 0 ? text : ''
        }
      }
      if (rowIndex === 0) alignments[column] = alignment
      column += colspan
    }
  })
  if (!grid.length) return ''
  const columns = grid.reduce((maximum, row) => Math.max(maximum, row.length), 0)
  const line = (row: string[]) => '| ' + Array.from({length: columns}, (_, index) => row[index] || '').join(' | ') + ' |'
  const separator = Array.from({length: columns}, (_, index) => ({
    left: ':---', center: ':---:', right: '---:',
  }[alignments[index]] || '---'))
  return [line(grid[0]), line(separator), ...grid.slice(1).map(line)].join('\n')
}

export function markdownToHtml(markdown: string): string {
  if (!markdown) return '<p></p>'
  if (markdown.startsWith('<')) return markdown
  return renderEditorMarkdown(markdown)
}

function isTaskList(element: HTMLElement): boolean {
  return element.dataset.type === 'taskList'
    || Array.from(element.children).some(isTaskItem)
}

function isTaskItem(element: Element): element is HTMLElement {
  return element.tagName.toUpperCase() === 'LI'
    && (element as HTMLElement).dataset.type === 'taskItem'
}

function serializeList(list: HTMLElement, indent = ''): string {
  const taskList = isTaskList(list)
  const ordered = list.tagName === 'OL'
  let number = Number.parseInt(list.getAttribute('start') || '1', 10)
  if (!Number.isFinite(number)) number = 1
  return Array.from(list.children).filter(item => item.tagName === 'LI').map(item => {
    const task = item as HTMLElement
    const checked = task.dataset.checked === 'true'
      || task.querySelector<HTMLInputElement>(':scope > label > input[type="checkbox"]')?.checked
    const prefix = taskList ? `- [${checked ? 'x' : ' '}] ` : ordered ? `${number++}. ` : '- '
    const content = taskList
      ? Array.from(task.children).find(child => child.tagName === 'DIV') || task : task
    // [ ] 是列表正文的一部分；Markdown 子列表缩进按 - 的标记宽度计算。
    const continuationIndent = indent + ' '.repeat(taskList ? 2 : prefix.length)
    const body: string[] = []
    for (const child of content.childNodes) {
      if (child instanceof HTMLElement && child.tagName === 'LABEL') continue
      if (child instanceof HTMLElement && ['UL', 'OL'].includes(child.tagName)) {
        body.push('\n' + serializeList(child, continuationIndent))
      } else {
        const text = nodeToMarkdown(child)
        if (text) body.push((body.length ? '\n\n' + continuationIndent : '')
          + text.replace(/\n/g, '\n' + continuationIndent))
      }
    }
    return (indent + prefix + body.join('')).trimEnd()
  }).join('\n')
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

function parseLocation(value?: string): DiaryLocation | null {
  if (!value) return null
  try {
    const parsed = JSON.parse(value) as Partial<DiaryLocation>
    if (parsed.coordinateSystem !== 'wgs84'
      || typeof parsed.latitude !== 'number'
      || typeof parsed.longitude !== 'number'
      || typeof parsed.capturedAt !== 'number') return null
    return parsed as DiaryLocation
  } catch {
    return null
  }
}

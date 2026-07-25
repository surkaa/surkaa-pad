/**
 * Tiptap HTML ↔ 结构化日记内容双向转换
 *
 * 存储格式 (Markdown): 纯文本 + [[TYPE:filename|...]] 附件标记
 * 编辑格式 (HTML):   Tiptap/ProseMirror 内部使用的 HTML
 */
import type { DiaryContent, DiaryContentNode } from '../../bindings'

// ==================== HTML → Markdown ====================

const INLINE_TAGS: Record<string, [string, string]> = {
  STRONG: ['**', '**'],
  B: ['**', '**'],
  EM: ['*', '*'],
  I: ['*', '*'],
  S: ['~~', '~~'],
  DEL: ['~~', '~~'],
  CODE: ['`', '`'],
  U: ['<u>', '</u>'],
};

export function htmlToMarkdown(html: string): string {
  if (!html) return '';

  const doc = new DOMParser().parseFromString(html, 'text/html');
  const parts: string[] = [];

  walkBlocks(doc.body, parts);

  return parts
    .join('\n\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

export function htmlToDiaryContent(html: string): DiaryContent {
  return markdownToDiaryContent(htmlToMarkdown(html))
}

export function diaryContentToHtml(
  content: DiaryContent,
  attachmentMap: Record<string, string>,
  attachmentFilenames: Record<string, string> = {},
): string {
  return markdownToHtml(diaryContentToMarkdown(content), attachmentMap, attachmentFilenames)
}

export function diaryContentToMarkdown(content: DiaryContent): string {
  return content.nodes.map(node => {
    switch (node.type) {
      case 'markdown':
        return node.text
      case 'image':
        return node.size === 'small'
          ? `[[IMG:${node.attachmentId}|size=small]]`
          : `[[IMG:${node.attachmentId}]]`
      case 'video':
        return `[[VID:${node.attachmentId}]]`
      case 'audio':
        return `[[AUD:${node.attachmentId}]]`
      case 'file':
        return `[[FILE:${node.attachmentId}]]`
      case 'album':
        return `[[ALBUM:${node.id}|mode=${node.displayMode}|images=${node.attachmentIds.map(encodeURIComponent).join(',')}]]`
    }
  }).join('')
}

export function markdownToDiaryContent(markdown: string): DiaryContent {
  const nodes: DiaryContentNode[] = []
  const re = /\[\[(IMG|VID|AUD|FILE|ALBUM):([^\]|]+)(?:\|([^\]]*))?\]\]/g
  let textStart = 0
  let match: RegExpExecArray | null

  while ((match = re.exec(markdown)) !== null) {
    if (match.index > textStart) {
      nodes.push({ type: 'markdown', text: markdown.slice(textStart, match.index) })
    }
    const [, type, attachmentId, config] = match
    switch (type) {
      case 'IMG':
        nodes.push({
          type: 'image',
          attachmentId,
          size: config?.split('|').includes('size=small') ? 'small' : 'normal',
        })
        break
      case 'VID':
        nodes.push({ type: 'video', attachmentId })
        break
      case 'AUD':
        nodes.push({ type: 'audio', attachmentId })
        break
      case 'FILE':
        nodes.push({ type: 'file', attachmentId })
        break
      case 'ALBUM': {
        const albumConfig = parseConfig(config)
        nodes.push({
          type: 'album',
          id: attachmentId,
          attachmentIds: (albumConfig.images || '')
            .split(',')
            .filter(Boolean)
            .map(decodeURIComponent),
          displayMode: albumConfig.mode === 'stackedCards' ? 'stackedCards' : 'horizontalList',
        })
        break
      }
    }
    textStart = re.lastIndex
  }

  if (textStart < markdown.length) {
    nodes.push({ type: 'markdown', text: markdown.slice(textStart) })
  }
  return { nodes }
}

function walkBlocks(parent: Node, out: string[]): void {
  for (const child of parent.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      const text = child.textContent || '';
      if (text.trim()) out.push(text.trim());
      continue;
    }

    if (child.nodeType !== Node.ELEMENT_NODE) continue;

    const el = child as HTMLElement;
    const tag = el.tagName.toUpperCase();

    // Attachment nodes — emit marker
    if (el.classList.contains('editor-image-album')) {
      const id = el.dataset.id || ''
      const mode = el.dataset.displayMode === 'stackedCards' ? 'stackedCards' : 'horizontalList'
      const images = parseJsonArray(el.dataset.images).map(encodeURIComponent).join(',')
      out.push(`[[ALBUM:${id}|mode=${mode}|images=${images}]]`)
      continue
    }
    if (tag === 'IMG' && el.dataset.id) {
      out.push(serializeImage(el));
      continue;
    }
    if (tag === 'VIDEO' && el.dataset.id) {
      out.push(`[[VID:${el.dataset.id}]]`);
      continue;
    }
    if (tag === 'AUDIO' && el.dataset.id) {
      out.push(`[[AUD:${el.dataset.id}]]`);
      continue;
    }
    if (el.classList.contains('editor-file-attachment')) {
      const id = el.dataset.id || '';
      out.push(`[[FILE:${id}]]`);
      continue;
    }

    switch (tag) {
      case 'BR':
        // handled inside inline serialization
        break;

      case 'P':
        out.push(serializeInline(el));
        break;

      case 'H1':
      case 'H2':
      case 'H3':
      case 'H4':
      case 'H5':
      case 'H6': {
        const level = parseInt(tag[1], 10);
        out.push('#'.repeat(level) + ' ' + serializeInline(el));
        break;
      }

      case 'UL':
      case 'OL': {
        const items: string[] = [];
        for (const li of el.children) {
          if (li.tagName.toUpperCase() === 'LI') {
            const prefix = tag === 'OL' ? `${items.length + 1}. ` : '- ';
            items.push(prefix + serializeInline(li));
          }
        }
        if (items.length) out.push(items.join('\n'));
        break;
      }

      case 'LI':
        // Already handled by UL/OL, but if orphaned
        out.push('- ' + serializeInline(el));
        break;

      case 'BLOCKQUOTE': {
        const text = serializeInline(el);
        out.push(text.split('\n').map(line => '> ' + line).join('\n'));
        break;
      }

      case 'HR':
        out.push('---');
        break;

      case 'PRE': {
        const code = el.querySelector('code');
        const codeText = (code || el).textContent || '';
        out.push('```\n' + codeText + '\n```');
        break;
      }

      case 'DIV': {
        // For divs that aren't attachment cards, recurse
        if (el.children.length > 0) {
          walkBlocks(el, out);
        } else {
          const text = serializeInline(el);
          if (text.trim()) out.push(text.trim());
        }
        break;
      }

      default:
        // Unknown block — treat content as text
        const text = serializeInline(el);
        if (text.trim()) out.push(text.trim());
        break;
    }
  }
}

function serializeImage(el: HTMLElement): string {
  const id = el.dataset.id || '';
  const size = el.dataset.size;
  if (size === 'small') {
    return `[[IMG:${id}|size=small]]`;
  }
  return `[[IMG:${id}]]`;
}

function serializeInline(parent: Node): string {
  let result = '';
  for (const child of parent.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += child.textContent || '';
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;

    const el = child as HTMLElement;
    const tag = el.tagName.toUpperCase();

    if (tag === 'BR') {
      result += '\n';
      continue;
    }

    // Nested block elements inside inline context (e.g., <p>text <strong>bold</strong></p>)
    const marker = INLINE_TAGS[tag];
    if (marker) {
      result += marker[0] + serializeInline(el) + marker[1];
    } else if (tag === 'A') {
      const href = el.getAttribute('href') || '';
      result += `[${serializeInline(el)}](${href})`;
    } else {
      result += serializeInline(el);
    }
  }
  return result;
}

// ==================== Markdown → HTML ====================

/** 匹配附件标记 [[TYPE:filename|...config]] */
const ATTACHMENT_RE = /\[\[(IMG|VID|AUD|FILE|ALBUM):([^\]|]+)(?:\|([^\]]*))?\]\]/g;

export function markdownToHtml(
  markdown: string,
  attachmentMap: Record<string, string>,
  attachmentFilenames: Record<string, string> = {},
): string {
  if (!markdown) return '<p></p>';
  if (markdown.startsWith('<')) return markdown; // 已是 HTML，原样返回

  // Step 1: Replace attachment markers with placeholder tokens
  const attachmentPlaceholders: Record<string, string> = {};
  let placeholderIndex = 0;

  let text = markdown.replace(ATTACHMENT_RE, (_match, type: string, attachmentId: string, config: string) => {
    const placeholder = ` ATT${placeholderIndex} `;
    const url = attachmentMap[attachmentId] || '';

    switch (type) {
      case 'IMG': {
        const sizeAttr = config?.includes('size=small') ? ' data-size="small"' : '';
        attachmentPlaceholders[placeholder] =
          `<img src="${escapeAttr(url)}" data-id="${escapeAttr(attachmentId)}"${sizeAttr}>`;
        break;
      }
      case 'VID':
        attachmentPlaceholders[placeholder] =
          `<video controls src="${escapeAttr(url)}" data-id="${escapeAttr(attachmentId)}"></video>`;
        break;
      case 'AUD':
        attachmentPlaceholders[placeholder] =
          `<audio controls src="${escapeAttr(url)}" data-id="${escapeAttr(attachmentId)}"></audio>`;
        break;
      case 'FILE':
        attachmentPlaceholders[placeholder] =
          `<div data-id="${escapeAttr(attachmentId)}" data-filename="${escapeAttr(attachmentFilenames[attachmentId] || attachmentId)}" class="editor-file-attachment"></div>`;
        break;
      case 'ALBUM': {
        const albumConfig = parseConfig(config)
        const images = (albumConfig.images || '')
          .split(',')
          .filter(Boolean)
          .map(decodeURIComponent)
        const urls = images.map(image => attachmentMap[image] || '')
        const mode = albumConfig.mode === 'stackedCards' ? 'stackedCards' : 'horizontalList'
        attachmentPlaceholders[placeholder] =
          `<div class="editor-image-album" data-id="${escapeAttr(attachmentId)}" data-images="${escapeAttr(JSON.stringify(images))}" data-display-mode="${mode}" data-urls="${escapeAttr(JSON.stringify(urls))}">`
          + images.map((image, index) =>
            `<img src="${escapeAttr(urls[index])}" data-id="${escapeAttr(image)}">`
          ).join('')
          + '</div>'
        break
      }
    }
    placeholderIndex++;
    return placeholder;
  });

  // Step 2: Split into blocks by blank lines
  const blocks = text.split(/\n{2,}/);
  const htmlBlocks: string[] = [];

  for (const block of blocks) {
    if (!block.trim()) continue;

    const trimmed = block.trim();

    // Check if it's an attachment placeholder
    if (/^ ATT\d+ $/.test(trimmed)) {
      htmlBlocks.push(attachmentPlaceholders[trimmed] || '');
      continue;
    }

    // Fenced code block
    if (/^```[\s\S]*```$/.test(trimmed)) {
      const code = trimmed.replace(/^```\n?/, '').replace(/\n?```$/, '');
      htmlBlocks.push(`<pre><code>${escapeHtml(code)}</code></pre>`);
      continue;
    }

    // Heading
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/m);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const content = processInline(headingMatch[2], attachmentPlaceholders);
      htmlBlocks.push(`<h${level}>${content}</h${level}>`);
      continue;
    }

    // HR
    if (/^---$/.test(trimmed) || /^\*\*\*$/.test(trimmed)) {
      htmlBlocks.push('<hr>');
      continue;
    }

    // Blockquote
    if (trimmed.startsWith('> ')) {
      const lines = trimmed.split('\n').map(l => l.replace(/^> /, '')).join('\n');
      htmlBlocks.push(`<blockquote><p>${processInline(lines, attachmentPlaceholders)}</p></blockquote>`);
      continue;
    }

    // Unordered list
    if (trimmed.match(/^- .+/)) {
      const items = trimmed.split('\n').filter(l => l.startsWith('- '));
      const listHtml = '<ul>' + items.map(item =>
        `<li><p>${processInline(item.slice(2), attachmentPlaceholders)}</p></li>`
      ).join('') + '</ul>';
      htmlBlocks.push(listHtml);
      continue;
    }

    // Ordered list
    if (trimmed.match(/^\d+\. .+/)) {
      const items = trimmed.split('\n').filter(l => /^\d+\. .+/.test(l));
      const listHtml = '<ol>' + items.map(item =>
        `<li><p>${processInline(item.replace(/^\d+\. /, ''), attachmentPlaceholders)}</p></li>`
      ).join('') + '</ol>';
      htmlBlocks.push(listHtml);
      continue;
    }

    // Regular paragraph with possible inline line breaks
    const lines = trimmed.split('\n');
    const paragraphContent = lines
      .map(line => processInline(line, attachmentPlaceholders))
      .join('<br>');
    htmlBlocks.push(`<p>${paragraphContent}</p>`);
  }

  const result = htmlBlocks.join('');
  return result || '<p></p>';
}

function processInline(
  text: string,
  placeholders: Record<string, string>,
): string {
  // Restore attachment placeholders
  let result = text.replace(/ ATT\d+ /g, (m) => placeholders[m] || m);

  // Inline code (process first to avoid conflicts)
  result = result.replace(/`([^`]+)`/g, '<code>$1</code>');

  // Bold
  result = result.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // Italic (single *, not double)
  result = result.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '<em>$1</em>');

  // Strikethrough
  result = result.replace(/~~([^~]+)~~/g, '<s>$1</s>');

  // Underline (non-standard, <u> tag)
  result = result.replace(/<u>([^<]*)<\/u>/g, '<u>$1</u>');

  // Links
  result = result.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');

  return result;
}

// ==================== Helpers ====================

function escapeAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function parseConfig(config?: string): Record<string, string> {
  return Object.fromEntries(
    (config || '')
      .split('|')
      .map(item => item.split('=', 2))
      .filter((item): item is [string, string] => item.length === 2),
  )
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

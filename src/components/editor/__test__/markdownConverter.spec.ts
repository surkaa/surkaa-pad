// @vitest-environment happy-dom
import { describe, it, expect } from 'vitest'
import {
  diaryContentToHtml,
  diaryContentToMarkdown,
  htmlToDiaryContent,
  htmlToMarkdown,
  markdownToDiaryContent,
  markdownToHtml,
} from '../markdownConverter'

// ==================== htmlToMarkdown ====================

describe('htmlToMarkdown', () => {
  it('converts plain paragraph', () => {
    expect(htmlToMarkdown('<p>hello world</p>')).toBe('hello world')
  })

  it('handles empty or whitespace-only input', () => {
    expect(htmlToMarkdown('')).toBe('')
    expect(htmlToMarkdown('<p></p>')).toBe('')
    expect(htmlToMarkdown('<p> </p>')).toBe('')
  })

  // --- Inline formatting ---

  it('converts bold (<strong> / <b>)', () => {
    expect(htmlToMarkdown('<p><strong>bold</strong> text</p>')).toBe('**bold** text')
    expect(htmlToMarkdown('<p><b>bold</b> text</p>')).toBe('**bold** text')
  })

  it('converts italic (<em> / <i>)', () => {
    expect(htmlToMarkdown('<p><em>italic</em> text</p>')).toBe('*italic* text')
    expect(htmlToMarkdown('<p><i>italic</i> text</p>')).toBe('*italic* text')
  })

  it('converts strikethrough (<s> / <del>)', () => {
    expect(htmlToMarkdown('<p><s>strike</s> text</p>')).toBe('~~strike~~ text')
    expect(htmlToMarkdown('<p><del>strike</del> text</p>')).toBe('~~strike~~ text')
  })

  it('converts inline code', () => {
    expect(htmlToMarkdown('<p>use <code>fn()</code> here</p>')).toBe('use `fn()` here')
  })

  it('converts underline', () => {
    expect(htmlToMarkdown('<p><u>underlined</u> text</p>')).toBe('<u>underlined</u> text')
  })

  it('converts links', () => {
    const result = htmlToMarkdown('<p><a href="https://x.com">link</a></p>')
    expect(result).toBe('[link](https://x.com)')
  })

  it('converts mixed inline formatting', () => {
    expect(htmlToMarkdown('<p><strong>bold</strong> and <em>italic</em></p>'))
      .toBe('**bold** and *italic*')
  })

  // --- Headings ---

  it('converts headings H1-H3', () => {
    expect(htmlToMarkdown('<h1>Title</h1>')).toBe('# Title')
    expect(htmlToMarkdown('<h2>Section</h2>')).toBe('## Section')
    expect(htmlToMarkdown('<h3>Sub</h3>')).toBe('### Sub')
  })

  it('converts heading with inline formatting', () => {
    expect(htmlToMarkdown('<h2>Hello <strong>World</strong></h2>')).toBe('## Hello **World**')
  })

  // --- Lists ---

  it('converts unordered list', () => {
    const html = '<ul><li><p>item 1</p></li><li><p>item 2</p></li></ul>'
    expect(htmlToMarkdown(html)).toBe('- item 1\n- item 2')
  })

  it('converts ordered list', () => {
    const html = '<ol><li><p>first</p></li><li><p>second</p></li></ol>'
    expect(htmlToMarkdown(html)).toBe('1. first\n2. second')
  })

  // --- Blockquotes ---

  it('converts blockquote', () => {
    expect(htmlToMarkdown('<blockquote><p>quoted</p></blockquote>')).toBe('> quoted')
  })

  it('converts multi-line blockquote', () => {
    const html = '<blockquote><p>line 1<br>line 2</p></blockquote>'
    expect(htmlToMarkdown(html)).toBe('> line 1\n> line 2')
  })

  // --- Horizontal rule ---

  it('converts horizontal rule', () => {
    expect(htmlToMarkdown('<hr>')).toBe('---')
  })

  // --- Code blocks ---

  it('converts fenced code block', () => {
    const html = '<pre><code>const x = 1;\nfn(x);</code></pre>'
    expect(htmlToMarkdown(html)).toBe('```\nconst x = 1;\nfn(x);\n```')
  })

  // --- Attachment markers ---

  it('converts image attachment to [[IMG:...]]', () => {
    const html = '<img src="blob:abc" data-id="photo.png">'
    expect(htmlToMarkdown(html)).toBe('[[IMG:photo.png]]')
  })

  it('converts small-size image with config', () => {
    const html = '<img src="blob:abc" data-id="photo.png" data-size="small">'
    expect(htmlToMarkdown(html)).toBe('[[IMG:photo.png|size=small]]')
  })

  it('converts video attachment', () => {
    const html = '<video controls src="blob:abc" data-id="clip.mp4"></video>'
    expect(htmlToMarkdown(html)).toBe('[[VID:clip.mp4]]')
  })

  it('converts audio attachment', () => {
    const html = '<audio controls src="blob:abc" data-id="recording.webm"></audio>'
    expect(htmlToMarkdown(html)).toBe('[[AUD:recording.webm]]')
  })

  it('converts file attachment', () => {
    const html = '<div data-id="doc.pdf" class="editor-file-attachment" contenteditable="false"></div>'
    expect(htmlToMarkdown(html)).toBe('[[FILE:doc.pdf]]')
  })

  // --- Multiple blocks ---

  it('joins blocks with double newlines', () => {
    const html = '<p>para 1</p><p>para 2</p>'
    expect(htmlToMarkdown(html)).toBe('para 1\n\npara 2')
  })

  it('collapses excessive blank lines', () => {
    const html = '<p>text</p><p></p><p></p><p></p><p>more</p>'
    expect(htmlToMarkdown(html)).toBe('text\n\nmore')
  })
})

// ==================== markdownToHtml ====================

describe('markdownToHtml', () => {
  const map: Record<string, string> = {
    'photo.png': 'blob:photo-url',
    'clip.mp4': 'blob:video-url',
    'recording.webm': 'blob:audio-url',
    'doc.pdf': '',
  }

  it('returns <p></p> for empty input', () => {
    expect(markdownToHtml('', {})).toBe('<p></p>')
  })

  it('returns input as-is when it starts with < (already HTML)', () => {
    expect(markdownToHtml('<p>already html</p>', {})).toBe('<p>already html</p>')
  })

  // --- Paragraph ---

  it('converts plain text to paragraph', () => {
    expect(markdownToHtml('hello', {})).toBe('<p>hello</p>')
  })

  // --- Headings ---

  it('converts heading markers', () => {
    expect(markdownToHtml('# Title', {})).toBe('<h1>Title</h1>')
    expect(markdownToHtml('## Section', {})).toBe('<h2>Section</h2>')
    expect(markdownToHtml('### Sub', {})).toBe('<h3>Sub</h3>')
  })

  // --- Inline formatting ---

  it('converts bold and italic', () => {
    expect(markdownToHtml('**bold** and *italic*', {}))
      .toBe('<p><strong>bold</strong> and <em>italic</em></p>')
  })

  it('converts strikethrough', () => {
    expect(markdownToHtml('~~strike~~', {})).toBe('<p><s>strike</s></p>')
  })

  it('converts inline code', () => {
    expect(markdownToHtml('use `fn()`', {})).toBe('<p>use <code>fn()</code></p>')
  })

  it('converts links', () => {
    expect(markdownToHtml('[text](https://x.com)', {}))
      .toBe('<p><a href="https://x.com">text</a></p>')
  })

  // --- Horizontal rule ---

  it('converts --- to hr', () => {
    expect(markdownToHtml('---', {})).toBe('<hr>')
  })

  it('converts *** to hr', () => {
    expect(markdownToHtml('***', {})).toBe('<hr>')
  })

  // --- Blockquote ---

  it('converts blockquote', () => {
    expect(markdownToHtml('> quoted text', {}))
      .toBe('<blockquote><p>quoted text</p></blockquote>')
  })

  // --- Lists ---

  it('converts unordered list', () => {
    expect(markdownToHtml('- a\n- b', {}))
      .toBe('<ul><li><p>a</p></li><li><p>b</p></li></ul>')
  })

  it('converts ordered list', () => {
    expect(markdownToHtml('1. first\n2. second', {}))
      .toBe('<ol><li><p>first</p></li><li><p>second</p></li></ol>')
  })

  // --- Code blocks ---

  it('converts fenced code block to pre>code', () => {
    const result = markdownToHtml('```\ncode here\n```', {})
    expect(result).toContain('<pre><code>')
    expect(result).toContain('code here')
    expect(result).toContain('</code></pre>')
  })

  // --- Attachment markers ---

  it('converts [[IMG:...]] to img element with src from map', () => {
    const result = markdownToHtml('[[IMG:photo.png]]', map)
    expect(result).toContain('<img src="blob:photo-url" data-id="photo.png">')
  })

  it('converts [[IMG:...|size=small]] with data-size attr', () => {
    const result = markdownToHtml('[[IMG:photo.png|size=small]]', map)
    expect(result).toContain('data-size="small"')
  })

  it('converts [[VID:...]] to video element', () => {
    const result = markdownToHtml('[[VID:clip.mp4]]', map)
    expect(result).toContain('<video controls src="blob:video-url" data-id="clip.mp4">')
  })

  it('converts [[AUD:...]] to audio element', () => {
    const result = markdownToHtml('[[AUD:recording.webm]]', map)
    expect(result).toContain('<audio controls src="blob:audio-url" data-id="recording.webm">')
  })

  it('converts [[FILE:...]] to div with class', () => {
    const result = markdownToHtml('[[FILE:doc.pdf]]', map)
    expect(result).toContain('<div data-id="doc.pdf" class="editor-file-attachment">')
  })

  it('uses empty src when attachment not in map', () => {
    const result = markdownToHtml('[[IMG:unknown.png]]', map)
    expect(result).toContain('src=""')
  })

  // --- Mixed content ---

  it('handles paragraph with attachment marker inline', () => {
    const result = markdownToHtml('before [[IMG:photo.png]] after', map)
    expect(result).toBe('<p>before <img src="blob:photo-url" data-id="photo.png"> after</p>')
  })

  it('handles multiple paragraphs with attachments', () => {
    const md = 'para one\n\n[[IMG:photo.png]]\n\npara two'
    const result = markdownToHtml(md, map)
    expect(result).toContain('<p>para one</p>')
    expect(result).toContain('<img src="blob:photo-url" data-id="photo.png">')
    expect(result).toContain('<p>para two</p>')
  })

  // --- Escaping ---

  it('escapes HTML entities in code blocks', () => {
    const result = markdownToHtml('```\n<script>\nalert(1)\n</script>\n```', {})
    // Code block content should be HTML-escaped
    expect(result).toContain('&lt;script&gt;')
    expect(result).not.toContain('<script>')
  })

  it('escapes quotes in attachment filenames', () => {
    const filename = 'file"x.png'
    const m: Record<string, string> = { [filename]: 'url' }
    const result = markdownToHtml(`[[IMG:${filename}]]`, m)
    // " in filename should be escaped within attribute
    expect(result).toContain('&quot;')
  })

  // --- Round-trip ---

  it('round-trips plain text through htmlToMarkdown and back', () => {
    const html = '<p>hello <strong>world</strong></p>'
    const md = htmlToMarkdown(html)
    const back = markdownToHtml(md, {})
    // Should reconstruct a similar structure
    expect(back).toContain('<strong>world</strong>')
  })

  it('round-trips attachment markers correctly', () => {
    const html = '<img src="blob:u" data-id="pic.png">'
    const md = htmlToMarkdown(html)
    expect(md).toBe('[[IMG:pic.png]]')
    const back = markdownToHtml(md, { 'pic.png': 'blob:u' })
    expect(back).toContain('src="blob:u"')
    expect(back).toContain('data-id="pic.png"')
  })
})

describe('structured diary content', () => {
  it('parses attachment markers into ordered nodes', () => {
    expect(markdownToDiaryContent('before [[IMG:photo.png|size=small]] after')).toEqual({
      nodes: [
        { type: 'markdown', text: 'before ' },
        { type: 'image', filename: 'photo.png', size: 'small' },
        { type: 'markdown', text: ' after' },
      ],
    })
  })

  it('serializes structured nodes to editor markdown', () => {
    expect(diaryContentToMarkdown({
      nodes: [
        { type: 'markdown', text: 'before\n\n' },
        { type: 'file', filename: 'doc.pdf' },
        { type: 'album', id: 'a1', images: ['1.jpg', '2.jpg'], displayMode: 'stackedCards' },
      ],
    })).toBe('before\n\n[[FILE:doc.pdf]][[ALBUM:a1|mode=stackedCards|images=1.jpg,2.jpg]]')
  })

  it('converts Tiptap HTML and structured content in both directions', () => {
    const content = htmlToDiaryContent(
      '<p>hello</p><img src="blob:u" data-id="photo.png" data-size="small">',
    )
    expect(content.nodes).toEqual([
      { type: 'markdown', text: 'hello\n\n' },
      { type: 'image', filename: 'photo.png', size: 'small' },
    ])
    expect(diaryContentToHtml(content, { 'photo.png': 'blob:u' }))
      .toContain('data-id="photo.png"')
  })
})

// @vitest-environment happy-dom
import {describe, expect, it} from 'vitest'
import {
  diaryContentToHtml,
  diaryContentToSource,
  htmlToDiaryContent,
  htmlToMarkdown,
  markdownToHtml,
} from '../markdownConverter'

describe('htmlToMarkdown', () => {
  it('converts a plain paragraph', () => {
    expect(htmlToMarkdown('<p>hello world</p>')).toBe('hello world')
  })

  it.each([
    ['bold', '<p><b>bold</b></p>', '**bold**'],
    ['italic', '<p><i>italic</i></p>', '*italic*'],
    ['deleted', '<p><del>deleted</del></p>', '~~deleted~~'],
    ['inline code', '<p><code>fn()</code></p>', '`fn()`'],
    ['underline', '<p><u>line</u></p>', '<u>line</u>'],
  ])('converts %s formatting', (_name, html, expected) => {
    expect(htmlToMarkdown(html)).toBe(expected)
  })

  it('converts paragraphs and inline formatting', () => {
    expect(htmlToMarkdown(
      '<p><strong>bold</strong> and <em>italic</em></p><p><s>strike</s> <code>code</code></p>',
    )).toBe('**bold** and *italic*\n\n~~strike~~ `code`')
  })

  it('converts headings, links and underline', () => {
    expect(htmlToMarkdown(
      '<h2>Hello <strong>World</strong></h2><p><a href="https://x.com">link</a> <u>line</u></p>',
    )).toBe('## Hello **World**\n\n[link](https://x.com) <u>line</u>')
  })

  it.each([
    ['h1', '<h1>Title</h1>', '# Title'],
    ['h2', '<h2>Title</h2>', '## Title'],
    ['h3', '<h3>Title</h3>', '### Title'],
  ])('converts %s headings', (_name, html, expected) => {
    expect(htmlToMarkdown(html)).toBe(expected)
  })

  it('converts ordered and unordered lists independently', () => {
    expect(htmlToMarkdown('<ul><li><p>a</p></li><li><p>b</p></li></ul>'))
      .toBe('- a\n- b')
    expect(htmlToMarkdown('<ol><li><p>a</p></li><li><p>b</p></li></ol>'))
      .toBe('1. a\n2. b')
  })

  it('converts a multiline quote', () => {
    expect(htmlToMarkdown('<blockquote><p>one<br>two</p></blockquote>'))
      .toBe('> one\n> two')
  })

  it('converts a horizontal rule and fenced code independently', () => {
    expect(htmlToMarkdown('<hr>')).toBe('---')
    expect(htmlToMarkdown('<pre><code>a\nb</code></pre>')).toBe('```\na\nb\n```')
  })

  it('converts lists, quote, rule and code block', () => {
    expect(htmlToMarkdown(
      '<ul><li><p>a</p></li><li><p>b</p></li></ul>'
      + '<ol><li><p>one</p></li><li><p>two</p></li></ol>'
      + '<blockquote><p>quoted<br>next</p></blockquote>'
      + '<hr><pre><code>const x = 1;</code></pre>',
    )).toBe(
      '- a\n- b\n\n1. one\n2. two\n\n> quoted\n> next\n\n---\n\n```\nconst x = 1;\n```',
    )
  })

  it('ignores attachment elements because they are not Markdown', () => {
    expect(htmlToMarkdown(
      '<p>before</p><img data-id="att-image"><video data-id="att-video"></video>'
      + '<audio data-id="att-audio"></audio>'
      + '<div class="editor-file-attachment" data-id="att-file"></div><p>after</p>',
    )).toBe('before\n\nafter')
  })

  it('handles empty input and collapses empty blocks', () => {
    expect(htmlToMarkdown('')).toBe('')
    expect(htmlToMarkdown('<p></p><p>text</p><p></p><p>more</p>')).toBe('text\n\nmore')
  })
})

describe('markdownToHtml', () => {
  it('converts a plain paragraph', () => {
    expect(markdownToHtml('hello')).toBe('<p>hello</p>')
  })

  it('keeps existing HTML unchanged', () => {
    expect(markdownToHtml('<p>already html</p>')).toBe('<p>already html</p>')
  })

  it.each([
    ['h1', '# Title', '<h1>Title</h1>'],
    ['h2', '## Title', '<h2>Title</h2>'],
    ['h3', '### Title', '<h3>Title</h3>'],
    ['strike', '~~gone~~', '<p><s>gone</s></p>'],
    ['inline code', '`fn()`', '<p><code>fn()</code></p>'],
    ['link', '[site](https://x.com)', '<p><a href="https://x.com">site</a></p>'],
    ['rule', '---', '<hr>'],
    ['alternative rule', '***', '<hr>'],
    ['quote', '> quote', '<blockquote><p>quote</p></blockquote>'],
  ])('converts %s Markdown', (_name, markdown, expected) => {
    expect(markdownToHtml(markdown)).toBe(expected)
  })

  it('converts common Markdown blocks', () => {
    const html = markdownToHtml(
      '# Title\n\n**bold** and *italic*\n\n- a\n- b\n\n> quote\n\n---',
    )
    expect(html).toContain('<h1>Title</h1>')
    expect(html).toContain('<p><strong>bold</strong> and <em>italic</em></p>')
    expect(html).toContain('<ul><li><p>a</p></li><li><p>b</p></li></ul>')
    expect(html).toContain('<blockquote><p>quote</p></blockquote>')
    expect(html).toContain('<hr>')
  })

  it('converts ordered lists and fenced code', () => {
    expect(markdownToHtml('1. one\n2. two'))
      .toBe('<ol><li><p>one</p></li><li><p>two</p></li></ol>')
    expect(markdownToHtml('```\n<script>\n```'))
      .toBe('<pre><code>&lt;script&gt;</code></pre>')
  })

  it('treats legacy-looking attachment markers as literal text', () => {
    expect(markdownToHtml('before [[IMG:att-fake]] after'))
      .toBe('<p>before [[IMG:att-fake]] after</p>')
    expect(markdownToHtml('[[ALBUM:a|images=1,2]]'))
      .toBe('<p>[[ALBUM:a|images=1,2]]</p>')
  })

  it('returns an empty paragraph for empty Markdown', () => {
    expect(markdownToHtml('')).toBe('<p></p>')
  })

  it('round-trips formatted Markdown text through HTML', () => {
    const markdown = 'hello **world** and *friend*'
    expect(htmlToMarkdown(markdownToHtml(markdown))).toBe(markdown)
  })
})

describe('structured diary content', () => {
  const attachmentMap = {
    'att-image': 'http://127.0.0.1/image',
    'att-video': 'http://127.0.0.1/video',
    'att-audio': 'http://127.0.0.1/audio',
    'att-1': 'http://127.0.0.1/1',
    '附件,2': 'http://127.0.0.1/2',
  }

  it('converts every structured attachment node directly to HTML', () => {
    const html = diaryContentToHtml({nodes: [
      {type: 'markdown', text: 'before'},
      {type: 'image', attachmentId: 'att-image', size: 'small'},
      {type: 'video', attachmentId: 'att-video'},
      {type: 'audio', attachmentId: 'att-audio'},
      {type: 'file', attachmentId: 'att-file'},
      {
        type: 'album',
        id: 'album-1',
        attachmentIds: ['att-1', '附件,2'],
        displayMode: 'stackedCards',
      },
    ]}, attachmentMap, {'att-file': '报告.pdf'})

    expect(html).toContain('<p>before</p>')
    expect(html).toContain('data-id="att-image" data-size="small"')
    expect(html).toContain('<video controls')
    expect(html).toContain('<audio controls')
    expect(html).toContain('data-filename="报告.pdf"')
    expect(html).toContain('class="editor-image-album"')
    expect(html).toContain('data-display-mode="stackedCards"')
  })

  it('uses an empty URL for an attachment missing from the map', () => {
    expect(diaryContentToHtml({nodes: [
      {type: 'image', attachmentId: 'missing', size: 'normal'},
    ]}, {})).toContain('src=""')
  })

  it('escapes special characters in attachment IDs and filenames', () => {
    const html = diaryContentToHtml({nodes: [
      {type: 'image', attachmentId: 'att-&quot"', size: 'normal'},
      {type: 'file', attachmentId: 'file-&',},
    ]}, {}, {'file-&': '报告 "最终".pdf'})
    expect(html).toContain('data-id="att-&amp;quot&quot;"')
    expect(html).toContain('data-filename="报告 &quot;最终&quot;.pdf"')
  })

  it('reads every Tiptap attachment element directly into ordered nodes', () => {
    const content = htmlToDiaryContent(
      '<p>before</p>'
      + '<img src="image" data-id="att-image" data-size="small">'
      + '<video src="video" data-id="att-video"></video>'
      + '<audio src="audio" data-id="att-audio"></audio>'
      + '<div class="editor-file-attachment" data-id="att-file" data-filename="报告.pdf"></div>'
      + '<div class="editor-image-album" data-id="album-1" '
      + 'data-images="[&quot;att-1&quot;,&quot;附件,2&quot;]" data-display-mode="stackedCards"></div>'
      + '<p>after</p>',
    )

    expect(content).toEqual({nodes: [
      {type: 'markdown', text: 'before'},
      {type: 'image', attachmentId: 'att-image', size: 'small'},
      {type: 'video', attachmentId: 'att-video'},
      {type: 'audio', attachmentId: 'att-audio'},
      {type: 'file', attachmentId: 'att-file'},
      {
        type: 'album',
        id: 'album-1',
        attachmentIds: ['att-1', '附件,2'],
        displayMode: 'stackedCards',
      },
      {type: 'markdown', text: 'after'},
    ]})
  })

  it('does not insert whitespace nodes between adjacent attachments', () => {
    expect(htmlToDiaryContent(
      '<img data-id="one"><img data-id="two"><audio data-id="three"></audio>',
    )).toEqual({nodes: [
      {type: 'image', attachmentId: 'one', size: 'normal'},
      {type: 'image', attachmentId: 'two', size: 'normal'},
      {type: 'audio', attachmentId: 'three'},
    ]})
  })

  it('combines contiguous text blocks but separates text around attachments', () => {
    expect(htmlToDiaryContent(
      '<p>one</p><p>two</p><video data-id="video"></video><p>three</p>',
    )).toEqual({nodes: [
      {type: 'markdown', text: 'one\n\ntwo'},
      {type: 'video', attachmentId: 'video'},
      {type: 'markdown', text: 'three'},
    ]})
  })

  it('defaults malformed album data to an empty horizontal album', () => {
    expect(htmlToDiaryContent(
      '<div class="editor-image-album" data-id="album" data-images="bad" data-display-mode="bad"></div>',
    )).toEqual({nodes: [{
      type: 'album',
      id: 'album',
      attachmentIds: [],
      displayMode: 'horizontalList',
    }]})
  })

  it('round-trips structured content without a text attachment protocol', () => {
    const content = {nodes: [
      {type: 'markdown' as const, text: 'literal [[IMG:not-an-attachment]]'},
      {type: 'image' as const, attachmentId: 'att-image', size: 'normal' as const},
      {
        type: 'album' as const,
        id: 'album-1',
        attachmentIds: ['att-1', '附件,2'],
        displayMode: 'horizontalList' as const,
      },
    ]}

    expect(htmlToDiaryContent(diaryContentToHtml(content, attachmentMap))).toEqual(content)
  })

  it('formats the actual V4 structure for source display', () => {
    const source = diaryContentToSource({nodes: [
      {type: 'image', attachmentId: 'att-image', size: 'normal'},
    ]})
    expect(source).toBe(JSON.stringify({nodes: [
      {type: 'image', attachmentId: 'att-image', size: 'normal'},
    ]}, null, 2))
    expect(source).not.toContain('[[IMG:')
  })
})

import MarkdownIt from 'markdown-it';

function createMarkdownParser(html: boolean) {
  const markdown = new MarkdownIt({html, breaks: true, linkify: false});

  // Tiptap 列表项以 paragraph 开头；紧凑列表也保留段落标签。
  markdown.renderer.rules.paragraph_open = () => '<p>';
  markdown.renderer.rules.paragraph_close = () => '</p>';
  markdown.renderer.rules.softbreak = () => '<br>';
  markdown.renderer.rules.hardbreak = () => '<br>';
  markdown.renderer.rules.fence = markdown.renderer.rules.code_block = (tokens, index) => {
    const token = tokens[index];
    const language = token.info.trim().split(/\s+/)[0];
    const className = language ? ` class="language-${markdown.utils.escapeHtml(language)}"` : '';
    return `<pre><code${className}>${markdown.utils.escapeHtml(token.content.replace(/\n$/, ''))}</code></pre>`;
  };
  // 普通 Markdown 图片不是日记附件，不自动加载远程 URL。
  markdown.renderer.rules.image = (tokens, index) =>
    `[图片：${markdown.utils.escapeHtml(tokens[index].content)}]`;

  markdown.core.ruler.after('inline', 'diary_task_lists', state => {
    const tokens = state.tokens;
    for (let index = 0; index < tokens.length; index++) {
      const list = tokens[index];
      if (list.type !== 'bullet_list_open') continue;
      const items: Array<{opening: typeof list; inline: typeof list; checked: boolean; prefixLength: number}> = [];
      let ordinaryItem = false;
      for (let child = index + 1; child < tokens.length; child++) {
        const token = tokens[child];
        if (token.type === 'bullet_list_close' && token.level === list.level) break;
        if (token.type !== 'list_item_open' || token.level !== list.level + 1) continue;
        const paragraph = tokens[child + 1];
        const inline = tokens[child + 2];
        const match = paragraph?.type === 'paragraph_open' && inline?.type === 'inline'
          ? inline.content.match(/^\[([ xX])\](?:[ \t]+|$)/) : null;
        if (!match) ordinaryItem = true;
        else items.push({opening: token, inline, checked: match[1].toLowerCase() === 'x', prefixLength: match[0].length});
      }
      if (ordinaryItem || !items.length) continue;
      list.attrSet('data-type', 'taskList');
      for (const item of items) {
        item.opening.attrSet('data-type', 'taskItem');
        item.opening.attrSet('data-checked', String(item.checked));
        item.inline.content = item.inline.content.slice(item.prefixLength);
        item.inline.children = state.md.parseInline(item.inline.content, state.env)[0].children;
      }
    }
  });

  return markdown;
}

const markdown = createMarkdownParser(true);
const clipboardMarkdown = createMarkdownParser(false);
const summaryMarkdown = createMarkdownParser(false);
// 折叠正文不经过 Tiptap 列表模型，可以直接显示混合普通项与待办项的 Markdown 列表。
summaryMarkdown.core.ruler.after('diary_task_lists', 'summary_mixed_tasks', state => {
  for (let index = 0; index < state.tokens.length; index++) {
    const item = state.tokens[index];
    const paragraph = state.tokens[index + 1];
    const inline = state.tokens[index + 2];
    if (item.type !== 'list_item_open' || paragraph?.type !== 'paragraph_open' || inline?.type !== 'inline') continue;
    const match = inline.content.match(/^\[([ xX])\](?:[ \t]+|$)/);
    if (!match) continue;
    item.attrSet('data-type', 'taskItem');
    item.attrSet('data-checked', String(match[1].toLowerCase() === 'x'));
    inline.content = inline.content.slice(match[0].length);
    inline.children = state.md.parseInline(inline.content, state.env)[0].children;
  }
});
summaryMarkdown.validateLink = value => {
  try { return ['http:', 'https:'].includes(new URL(value).protocol); }
  catch { return false; }
};
summaryMarkdown.renderer.rules.link_open = (tokens, index, options, _env, renderer) => {
  tokens[index].attrSet('target', '_blank');
  tokens[index].attrSet('rel', 'noopener noreferrer');
  return renderer.renderToken(tokens, index, options);
};

/** 折叠正文只解释 Markdown，不执行 HTML，也不自动加载远程图片或嵌套折叠节点。 */
export function renderSummaryMarkdown(source: string): string {
  return summaryMarkdown.render(source).trim();
}

export function renderEditorMarkdown(source: string): string {
  return markdown.render(source).replace(/>\n+(?=<)/g, '>').trim() || '<p></p>';
}

/** 识别 Markdown 列表和表格；代码块中的示例不转换，剪贴板 HTML 不执行。 */
export function renderPastedEditorMarkdown(source: string): string | null {
  const env = {};
  const tokens = clipboardMarkdown.parse(source, env);
  if (!tokens.some(token => ['table_open', 'bullet_list_open', 'ordered_list_open'].includes(token.type))) return null;
  return clipboardMarkdown.renderer.render(tokens, clipboardMarkdown.options, env)
    .replace(/>\n+(?=<)/g, '>').trim();
}

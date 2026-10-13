import MarkdownIt from 'markdown-it';

const markdown = new MarkdownIt({html: true, breaks: true, linkify: false}).disable('table');

// Tiptap 列表项以 paragraph 开头；紧凑列表也保留段落标签。
markdown.renderer.rules.paragraph_open = () => '<p>';
markdown.renderer.rules.paragraph_close = () => '</p>';
markdown.renderer.rules.softbreak = () => '<br>';
markdown.renderer.rules.hardbreak = () => '<br>';
markdown.renderer.rules.fence = markdown.renderer.rules.code_block = (tokens, index) =>
  `<pre><code>${markdown.utils.escapeHtml(tokens[index].content.replace(/\n$/, ''))}</code></pre>`;
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

export function renderEditorMarkdown(source: string): string {
  return markdown.render(source).replace(/>\n+(?=<)/g, '>').trim() || '<p></p>';
}

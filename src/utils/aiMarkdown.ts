import MarkdownIt from 'markdown-it';

const markdown = new MarkdownIt({
  html: false,
  breaks: true,
  linkify: true,
  typographer: false,
});

markdown.validateLink = (url: string) => {
  try {
    return ['http:', 'https:'].includes(new URL(url).protocol);
  } catch {
    return false;
  }
};

const defaultLinkOpen = markdown.renderer.rules.link_open
  ?? ((tokens, index, options, _env, renderer) => renderer.renderToken(tokens, index, options));

markdown.renderer.rules.link_open = (tokens, index, options, env, renderer) => {
  tokens[index].attrSet('rel', 'noreferrer noopener');
  return defaultLinkOpen(tokens, index, options, env, renderer);
};

// 不让模型输出的 Markdown 自动加载远程图片，避免产生未经过用户确认的网络请求。
markdown.renderer.rules.image = (tokens, index) => {
  const alt = tokens[index].content.trim() || '图片';
  return `<span class="ai-markdown-image-placeholder">[图片：${markdown.utils.escapeHtml(alt)}]</span>`;
};

export function renderAiMarkdown(source: string): string {
  return markdown.render(source);
}

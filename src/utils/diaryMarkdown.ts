import type {AttachmentMeta, DiaryContent} from '../bindings';

/** 复制可读的 Markdown；附件只使用相对链接，不泄露临时服务地址或访问令牌。 */
export function diaryContentToMarkdown(
  content: DiaryContent,
  attachments: readonly Pick<AttachmentMeta, 'id' | 'filename'>[] = [],
): string {
  const filenames = new Map(attachments.map(attachment => [attachment.id, attachment.filename]));
  const attachmentLink = (id: string, image = false) => {
    const filename = filenames.get(id) || id;
    return `${image ? '!' : ''}[${escapeLabel(filename)}](attachments/${encodeURIComponent(filename)})`;
  };

  return content.nodes.map(node => {
    switch (node.type) {
      case 'markdown': return node.text;
      case 'summary':
        return `<details>\n<summary>${escapeHtml(node.summary)}</summary>\n\n`
          + `${diaryContentToMarkdown({nodes: node.content}, attachments)}\n\n</details>`;
      case 'location': {
        const location = node.location;
        const accuracy = location.horizontalAccuracyMeters === null
          ? '' : ` · 精度约 ±${Math.round(location.horizontalAccuracyMeters)} 米`;
        return `📍 ${escapeLabel(location.placeName || '未命名地点')}：`
          + `${location.latitude}, ${location.longitude}（WGS-84）${accuracy}`;
      }
      case 'image': return attachmentLink(node.attachmentId, true);
      case 'album': return node.attachmentIds.map(id => attachmentLink(id, true)).join('\n\n');
      case 'video':
      case 'audio':
      case 'file': return attachmentLink(node.attachmentId);
    }
  }).filter(block => block.trim()).join('\n\n');
}

function escapeLabel(value: string): string {
  return value.replace(/[\\\[\]*_`]/g, '\\$&').replace(/\r?\n/g, ' ');
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

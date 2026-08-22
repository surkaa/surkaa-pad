import {Fragment, type Node as ProseMirrorNode} from '@tiptap/pm/model';
import type {EditorState, Transaction} from '@tiptap/pm/state';
import type {AttachmentMeta} from '../../bindings';
import type {EditorJsonNode} from './albumEditor';

export interface DiaryBlockDescriptor {
  sourceIndex: number;
  type: string;
  title: string;
  preview: string;
  icon: string;
}

type AttachmentLabel = Pick<AttachmentMeta, 'id' | 'filename'>;

const TEXT_BLOCKS: Record<string, {title: string; icon: string}> = {
  paragraph: {title: '段落', icon: 'notes'},
  heading: {title: '标题', icon: 'title'},
  bulletList: {title: '无序列表', icon: 'format_list_bulleted'},
  orderedList: {title: '有序列表', icon: 'format_list_numbered'},
  taskList: {title: '待办列表', icon: 'checklist'},
  blockquote: {title: '引用', icon: 'format_quote'},
  codeBlock: {title: '代码块', icon: 'code'},
  horizontalRule: {title: '分隔线', icon: 'horizontal_rule'},
};

export function describeDiaryBlocks(
  document: EditorJsonNode,
  attachments: readonly AttachmentLabel[],
): DiaryBlockDescriptor[] {
  const filenames = new Map(attachments.map(attachment => [attachment.id, attachment.filename]));
  return (document.content || []).map((node, sourceIndex) => {
    const structured = describeStructuredNode(node, filenames);
    if (structured) return {sourceIndex, type: node.type, ...structured};

    const fallback = TEXT_BLOCKS[node.type] || {title: '内容块', icon: 'widgets'};
    const headingLevel = node.type === 'heading' && typeof node.attrs?.level === 'number'
      ? ` H${node.attrs.level}`
      : '';
    return {
      sourceIndex,
      type: node.type,
      title: `${fallback.title}${headingLevel}`,
      preview: nodeText(node) || (node.type === 'paragraph' ? '空白段落' : '无文字内容'),
      icon: fallback.icon,
    };
  });
}

export function moveDiaryBlock<T>(
  blocks: readonly T[],
  fromIndex: number,
  toIndex: number,
): T[] {
  if (
    !Number.isInteger(fromIndex)
    || !Number.isInteger(toIndex)
    || fromIndex < 0
    || fromIndex >= blocks.length
    || toIndex < 0
    || toIndex >= blocks.length
    || fromIndex === toIndex
  ) {
    return [...blocks];
  }
  const next = [...blocks];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

export function isValidBlockOrder(order: readonly number[], blockCount: number): boolean {
  return order.length === blockCount
    && order.every(index => Number.isInteger(index) && index >= 0 && index < blockCount)
    && new Set(order).size === blockCount;
}

export function createBlockOrderTransaction(
  state: EditorState,
  order: readonly number[],
): Transaction | null {
  const nodes: ProseMirrorNode[] = [];
  state.doc.forEach(node => nodes.push(node));
  if (!isValidBlockOrder(order, nodes.length)) return null;
  if (order.every((sourceIndex, index) => sourceIndex === index)) return null;

  const reordered = order.map(sourceIndex => nodes[sourceIndex]);
  return state.tr.replaceWith(
    0,
    state.doc.content.size,
    Fragment.fromArray(reordered),
  );
}

export function topLevelBlockIdentities(document: EditorJsonNode): string[] {
  return (document.content || []).map(blockIdentity);
}

function describeStructuredNode(
  node: EditorJsonNode,
  filenames: ReadonlyMap<string, string>,
): Omit<DiaryBlockDescriptor, 'sourceIndex' | 'type'> | null {
  const attachmentId = typeof node.attrs?.id === 'string' ? node.attrs.id : '';
  const filename = filenames.get(attachmentId) || attachmentId;
  switch (node.type) {
    case 'imageNode':
      return {title: '图片', preview: filename || '未命名图片', icon: 'image'};
    case 'videoNode':
      return {title: '视频', preview: filename || '未命名视频', icon: 'video_library'};
    case 'audioNode':
      return {title: '音频', preview: filename || '未命名音频', icon: 'audiotrack'};
    case 'fileNode':
      return {title: '文件', preview: filename || '未命名文件', icon: 'attach_file'};
    case 'albumNode': {
      const images = stringArray(node.attrs?.images);
      const firstFilename = images.length > 0
        ? filenames.get(images[0]) || images[0]
        : '';
      return {
        title: '图集',
        preview: `${images.length} 张图片${firstFilename ? ` · ${firstFilename}` : ''}`,
        icon: 'collections',
      };
    }
    case 'summaryNode':
      return {
        title: '折叠内容',
        preview: stringAttr(node.attrs?.summary) || '未命名折叠内容',
        icon: 'unfold_more',
      };
    case 'locationNode': {
      const location = objectAttr(node.attrs?.location);
      const placeName = stringAttr(location?.placeName);
      const latitude = numberAttr(location?.latitude);
      const longitude = numberAttr(location?.longitude);
      return {
        title: '位置',
        preview: placeName || (
          latitude !== null && longitude !== null
            ? `${latitude.toFixed(6)}, ${longitude.toFixed(6)}`
            : '未命名地点'
        ),
        icon: 'location_on',
      };
    }
    default:
      return null;
  }
}

function blockIdentity(node: EditorJsonNode): string {
  const id = typeof node.attrs?.id === 'string' ? node.attrs.id : '';
  if ([
    'imageNode',
    'videoNode',
    'audioNode',
    'fileNode',
    'albumNode',
  ].includes(node.type)) {
    return `${node.type}:${id}`;
  }
  return `${node.type}:${JSON.stringify(node)}`;
}

function nodeText(node: EditorJsonNode): string {
  const text = typeof node.text === 'string' ? node.text : '';
  const nested = (node.content || []).map(nodeText).filter(Boolean).join(' ');
  return [text, nested]
    .filter(Boolean)
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter(item => typeof item === 'string') : [];
}

function stringAttr(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function numberAttr(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function objectAttr(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object'
    ? value as Record<string, unknown>
    : null;
}

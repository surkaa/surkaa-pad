import {Node as TiptapNode, mergeAttributes} from '@tiptap/vue-3';
import type {Node as ProseMirrorNode} from '@tiptap/pm/model';
import type {NodeView} from '@tiptap/pm/view';

export interface SummaryAttributes {
  summary: string;
  content: string;
}

export interface SummaryNodeOptions {
  onEdit: (position: number, attrs: SummaryAttributes) => void;
}

declare module '@tiptap/vue-3' {
  interface Commands<ReturnType> {
    summaryNode: {
      insertSummary: (attrs: SummaryAttributes) => ReturnType;
    };
  }
}

function createSummaryNodeView(
  initialNode: ProseMirrorNode,
  getPos: () => number | undefined,
  onEdit: SummaryNodeOptions['onEdit'],
): NodeView {
  let currentNode = initialNode;
  const dom = document.createElement('details');
  const summary = document.createElement('summary');
  const summaryText = document.createElement('span');
  const content = document.createElement('div');
  const editButton = document.createElement('button');
  let toggleAnimation: Animation | null = null;

  dom.className = 'editor-summary';
  dom.contentEditable = 'false';
  content.className = 'editor-summary-content';
  editButton.type = 'button';
  editButton.className = 'editor-summary-edit';
  editButton.title = '编辑折叠内容';
  editButton.setAttribute('aria-label', '编辑折叠内容');
  editButton.textContent = '✎';
  summary.append(summaryText, editButton);
  dom.append(summary, content);

  const sync = (node: ProseMirrorNode) => {
    currentNode = node;
    dom.dataset.summary = node.attrs.summary;
    dom.dataset.content = node.attrs.content;
    summaryText.textContent = node.attrs.summary;
    content.textContent = node.attrs.content;
  };
  sync(initialNode);

  const handleEdit = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
    const position = getPos();
    if (typeof position === 'number') {
      onEdit(position, {
        summary: currentNode.attrs.summary,
        content: currentNode.attrs.content,
      });
    }
  };
  editButton.addEventListener('click', handleEdit);

  const disableDraggingForContentSelection = () => {
    dom.draggable = false;
  };
  const enableDraggingFromSummary = () => {
    dom.draggable = true;
  };
  content.addEventListener('pointerdown', disableDraggingForContentSelection);
  content.addEventListener('mousedown', disableDraggingForContentSelection);
  summary.addEventListener('pointerdown', enableDraggingFromSummary);
  summary.addEventListener('mousedown', enableDraggingFromSummary);

  const handleToggle = (event: MouseEvent) => {
    if (editButton.contains(event.target as Node)) return;
    event.preventDefault();
    if (toggleAnimation) return;

    const shouldOpen = !dom.open;
    const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
    if (reduceMotion || typeof content.animate !== 'function') {
      dom.open = shouldOpen;
      return;
    }

    if (shouldOpen) dom.open = true;
    const computedStyle = window.getComputedStyle(content);
    const expandedFrame: Keyframe = {
      height: `${content.scrollHeight}px`,
      opacity: 1,
      paddingTop: computedStyle.paddingTop,
      paddingBottom: computedStyle.paddingBottom,
    };
    const collapsedFrame: Keyframe = {
      height: '0px',
      opacity: 0,
      paddingTop: '0px',
      paddingBottom: '0px',
    };

    toggleAnimation = content.animate(
      shouldOpen ? [collapsedFrame, expandedFrame] : [expandedFrame, collapsedFrame],
      {duration: 180, easing: 'cubic-bezier(0.2, 0, 0, 1)'},
    );
    toggleAnimation.onfinish = () => {
      if (!shouldOpen) dom.open = false;
      toggleAnimation = null;
    };
    toggleAnimation.oncancel = () => {
      toggleAnimation = null;
    };
  };
  summary.addEventListener('click', handleToggle);

  return {
    dom,
    update(node) {
      if (node.type !== currentNode.type) return false;
      sync(node);
      return true;
    },
    stopEvent: event => {
      const target = event.target as Node;
      return editButton.contains(target) || content.contains(target);
    },
    ignoreMutation: () => true,
    destroy() {
      toggleAnimation?.cancel();
      editButton.removeEventListener('click', handleEdit);
      content.removeEventListener('pointerdown', disableDraggingForContentSelection);
      content.removeEventListener('mousedown', disableDraggingForContentSelection);
      summary.removeEventListener('pointerdown', enableDraggingFromSummary);
      summary.removeEventListener('mousedown', enableDraggingFromSummary);
      summary.removeEventListener('click', handleToggle);
    },
  };
}

export const SummaryNode = TiptapNode.create<SummaryNodeOptions>({
  name: 'summaryNode',

  group: 'block',
  atom: true,
  selectable: true,
  draggable: true,

  addOptions() {
    return {
      onEdit: () => undefined,
    };
  },

  addAttributes() {
    return {
      summary: {default: ''},
      content: {default: ''},
    };
  },

  parseHTML() {
    return [{
      tag: 'details.editor-summary',
      getAttrs: element => {
        const details = element as HTMLElement;
        return {
          summary: details.dataset.summary
            ?? details.querySelector('summary')?.textContent
            ?? '',
          content: details.dataset.content
            ?? details.querySelector('.editor-summary-content')?.textContent
            ?? '',
        };
      },
    }];
  },

  renderHTML({node}) {
    return [
      'details',
      mergeAttributes({
        class: 'editor-summary',
        'data-summary': node.attrs.summary,
        'data-content': node.attrs.content,
        contenteditable: 'false',
      }),
      ['summary', node.attrs.summary],
      ['div', {class: 'editor-summary-content'}, node.attrs.content],
    ];
  },

  addNodeView() {
    return ({node, getPos}) => createSummaryNodeView(node, getPos, this.options.onEdit);
  },

  addCommands() {
    return {
      insertSummary: attrs => ({commands}) => commands.insertContent({
        type: this.name,
        attrs,
      }),
    };
  },
});

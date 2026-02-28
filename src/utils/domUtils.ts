export type MediaType = 'img' | 'video' | 'audio';

/**
 * DOM 节点插入器
 */
export const insertMediaNode = (editor: HTMLElement | undefined, nodeType: MediaType, url: string, filename: string, range: Range | null) => {
    if (!editor) return;

    const el = document.createElement(nodeType);
    el.src = url;
    el.dataset.id = filename;

    if (nodeType !== 'img') {
        (el as HTMLMediaElement).controls = true;
    }

    insertBlockNode(editor, el, range);
};

/**
 * 在编辑器中安全插入一个块级元素
 */
const insertBlockNode = (
    editor: HTMLElement,
    mediaNode: HTMLElement,
    savedRange?: Range | null
): Range => {
    const selection = window.getSelection();
    // 优先使用传入的固化选区
    const range = savedRange || (selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null);

    if (!range) {
        editor.appendChild(mediaNode);
        const p = createEmptyParagraph();
        editor.appendChild(p);
        editor.dispatchEvent(new Event('input', {bubbles: true}));
        return setCursorTo(p);
    }

    let node: Node | null = range.commonAncestorContainer;

    // 防御：光标跑到了编辑器外部
    if (!editor.contains(node)) {
        editor.appendChild(mediaNode);
        const p = createEmptyParagraph();
        editor.appendChild(p);
        editor.dispatchEvent(new Event('input', {bubbles: true}));
        return setCursorTo(p);
    }

    let currentBlock: HTMLElement | null = null;

    if (node === editor) {
        // 修复：当光标刚好在两个块级元素之间时，利用 offset 精准定位
        const targetNode = editor.childNodes[range.startOffset];
        if (targetNode) {
            editor.insertBefore(mediaNode, targetNode);
        } else {
            editor.appendChild(mediaNode);
        }
    } else {
        // 向上查找直到找到 editor 的直接子节点
        while (node && node.parentNode !== editor) {
            node = node.parentNode;
        }
        currentBlock = node as HTMLElement;

        if (!currentBlock) {
            editor.appendChild(mediaNode);
        } else {
            // 插入到当前所在块的“后面”
            if (currentBlock.nextSibling) {
                editor.insertBefore(mediaNode, currentBlock.nextSibling);
            } else {
                editor.appendChild(mediaNode);
            }
        }
    }

    // 插入落脚点
    const nextBlock = createEmptyParagraph();
    if (mediaNode.nextSibling) {
        editor.insertBefore(nextBlock, mediaNode.nextSibling);
    } else {
        editor.appendChild(nextBlock);
    }

    editor.dispatchEvent(new Event('input', {bubbles: true}));
    // 返回新的 Range，为连续多图插入提供上下文
    return setCursorTo(nextBlock);
};

const createEmptyParagraph = () => {
    const div = document.createElement('div');
    div.innerHTML = '<br>';
    return div;
};

// 改造原有的 setCursorTo 使其返回 Range 对象
const setCursorTo = (node: Node): Range => {
    const range = document.createRange();
    const selection = window.getSelection();

    range.selectNodeContents(node);
    range.collapse(false);

    selection?.removeAllRanges();
    selection?.addRange(range);
    return range;
};

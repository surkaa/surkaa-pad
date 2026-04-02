import {Ref} from "vue";

export type MediaType = 'img' | 'video' | 'audio';
export type DatasetConfig = Record<string, string>;

export const useDomInsert = (editorRef: Ref<HTMLElement | undefined>) => {

    /**
     * 插入通用文件节点 (原子节点)
     */
    function insertFileNode(filename: string, filesizeText: string, range: Range | null) {
        if (!editorRef.value) return;

        const el = document.createElement('div');
        el.className = 'editor-file-attachment';
        el.dataset.id = filename;
        el.contentEditable = 'false';

        el.innerHTML = `<div class="file-title"><span class="file-icon">📎</span><span class="file-name">${filename}</span></div><span class="file-size">${filesizeText}</span>`;

        insertBlockNode(editorRef.value, el, range);
    }

    /**
     * DOM 节点插入器
     */
    function insertMediaNode(nodeType: MediaType, url: string, filename: string, range: Range | null, dataset?: DatasetConfig) {
        if (!editorRef.value) return;

        const el = document.createElement(nodeType);
        el.src = url;
        el.dataset.id = filename;

        if (dataset) {
            for (const key in dataset) {
                el.dataset[key] = dataset[key];
            }
        }

        if (nodeType !== 'img') {
            (el as HTMLMediaElement).controls = true;
        }

        insertBlockNode(editorRef.value, el, range);
    }

    /**
     * 在编辑器中安全插入一个块级元素
     */
    function insertBlockNode(
        editor: HTMLElement,
        mediaNode: HTMLElement,
        savedRange?: Range | null
    ): Range {
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
    }

    function createEmptyParagraph() {
        const div = document.createElement('div');
        div.innerHTML = '<br>';
        return div;
    }

    function setCursorTo(node: Node): Range {
        const range = document.createRange();
        const selection = window.getSelection();

        range.selectNodeContents(node);
        range.collapse(false);

        selection?.removeAllRanges();
        selection?.addRange(range);
        return range;
    }

    return {
        insertFileNode,
        insertMediaNode
    }
}
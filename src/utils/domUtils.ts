/**
 * 在编辑器中安全插入一个块级元素
 * 替代 document.execCommand('insertHTML')
 * * 结构目标：
 * <当前块>...</当前块>
 * <wrapper>媒体元素</wrapper>  <-- 插入这里
 * <div><br></div>            <-- 插入这里 (光标位置)
 */
export const insertBlockNode = (editor: HTMLElement, mediaNode: HTMLElement) => {
    console.log('准备插入媒体元素:', mediaNode);
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;

    const range = selection.getRangeAt(0);

    // 1. 寻找光标所在的顶级块元素 (直接在 editor 下一级的 div 或 p)
    let currentBlock = range.commonAncestorContainer as HTMLElement;

    // 向上查找直到找到 editor 的直接子节点
    while (currentBlock && currentBlock.parentElement !== editor) {
        currentBlock = currentBlock.parentElement as HTMLElement;
    }

    // 如果找不到（比如编辑器是空的），就创建一个新块
    if (!currentBlock) {
        // 编辑器可能是空的，直接追加
        editor.appendChild(mediaNode);
        // 追加光标占位符
        const p = createEmptyParagraph();
        editor.appendChild(p);
        setCursorTo(p);
        // 手动触发 input 事件
        editor.dispatchEvent(new Event('input', { bubbles: true }));
        return;
    }

    // 2. 核心逻辑：把新元素插入到当前块的“后面”
    // 这样保证了媒体元素永远不会被吞进上面的 div 里
    if (currentBlock.nextSibling) {
        editor.insertBefore(mediaNode, currentBlock.nextSibling);
    } else {
        editor.appendChild(mediaNode);
    }

    // 3. 插入光标落脚点 (一个新的空行)
    const nextBlock = createEmptyParagraph();
    if (mediaNode.nextSibling) {
        editor.insertBefore(nextBlock, mediaNode.nextSibling);
    } else {
        editor.appendChild(nextBlock);
    }

    // 4. 将光标移动到新的空行里
    setCursorTo(nextBlock);

    // 5. 手动触发 input 事件
    editor.dispatchEvent(new Event('input', { bubbles: true }));
};

// 创建一个标准的空行 div
const createEmptyParagraph = () => {
    const div = document.createElement('div');
    div.innerHTML = '<br>'; // 必须有个 br，否则撑不开高度
    return div;
};

// 设置光标位置
const setCursorTo = (node: Node) => {
    const range = document.createRange();
    const selection = window.getSelection();

    // 聚焦到内部
    range.selectNodeContents(node);
    range.collapse(false); // 光标放到末尾

    selection?.removeAllRanges();
    selection?.addRange(range);
};

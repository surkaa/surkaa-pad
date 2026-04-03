import {Extension, ExtensionContext, MenuButton} from "./extension.ts";
import {formatBytes} from "../../utils";
import {getFilename} from "./utils.ts";

// 根据固定的 class 识别文件节点
function match(node: Node) {
    return node.nodeName === 'DIV' && (node as HTMLElement).classList.contains('editor-file-attachment');
}

function getMark(filename: string) {
    return `[[FILE:${filename}]]`;
}

function hasMark(source: string, filename: string) {
    return new RegExp(`\\[\\[FILE:${filename}\\]\\]`).test(source);
}

function toHtml(source: string, ctx: ExtensionContext) {
    return source.replace(/\[\[FILE:([^\]]+)]]/g, (_, filename) => {
        const att = ctx.getAttachment(filename);
        let filesizeText;
        if (att) {
            filesizeText = formatBytes(att.size);
        } else {
            filesizeText = '未知大小';
        }

        // 内部 DOM 结构：图标 + 文件名
        return `<div data-id="${filename}" contenteditable="false" class="editor-file-attachment"><div class="file-title"><span class="file-icon">📎</span><span class="file-name">${filename}</span></div><span class="file-size">${filesizeText}</span></div>`;
    });
}

function serialize(n: HTMLElement) {
    const filename = getFilename(n);
    if (!filename) return '';
    return `[[FILE:${filename}]]`;
}

function onClick(_e: MouseEvent, node: HTMLDivElement, ctx: ExtensionContext) {
    const filename = getFilename(node);
    if (!filename) {
        console.error(`无法打开附件，缺少 data-id`);
        return;
    }
    const att = ctx.getAttachment(filename);
    console.log(`Attachment: ${att?.filename}`);
}

function onContextmenu(_: MouseEvent, node: HTMLDivElement, ctx: ExtensionContext): MenuButton<HTMLDivElement>[] {
    const filename = getFilename(node);
    if (!filename) {
        console.error(`无法打开附件菜单，缺少 data-id`);
        return [];
    }
    return [{
        label: '重命名附件',
        action: (el) => ctx.emit('renameAttachment', filename, (newFilename: string) => {
            // 更新DOM显示的文件
            const span = el.querySelector('.file-name');
            if (span) {
                span.textContent = newFilename;
            }
        })
    }];
}

export const FileExtension: Extension<HTMLDivElement> = {
    name: "file",
    match, getMark, hasMark, toHtml, serialize, onClick, onContextmenu, getFilename
}

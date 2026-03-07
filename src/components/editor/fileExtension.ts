import { Extension } from "./extension.ts";
import {formatBytes} from "../../utils";

export const FileExtension: Extension<HTMLDivElement> = {
    name: "file",

    // 根据固定的 class 识别文件节点
    match: (node) => node.nodeName === 'DIV' && (node as HTMLElement).classList.contains('editor-file-attachment'),

    getMark: (filename) => `[[FILE:${filename}]]`,

    hasMark: (source, filename) => new RegExp(`\\[\\[FILE:${filename}\\]\\]`).test(source),

    toHtml: (source, ctx) => source.replace(/\[\[FILE:([^\]]+)]]/g, (_, filename) => {
        const att = ctx.getAttachment(filename);
        let filesizeText;
        if (att) {
            filesizeText = formatBytes(att.size);
        } else {
            filesizeText = '未知大小';
        }

        // 内部 DOM 结构：图标 + 文件名
        return `<div data-id="${filename}" contenteditable="false" class="editor-file-attachment"><div class="file-title"><span class="file-icon">📎</span><span class="file-name">${filename}</span></div><span class="file-size">${filesizeText}</span></div>`;
    }),

    serialize: (n: HTMLElement) => {
        const filename = n.dataset.id;
        if (!filename) return '';
        return `[[FILE:${filename}]]`;
    },

    onClick: (_e, node, ctx) => {
        const filename = node.dataset.id;
        if (!filename) {
            console.error(`无法打开附件，缺少 data-id`);
            return;
        }
        const att = ctx.getAttachment(filename);
        console.log(`Attachment: ${att?.filename}`);
    },

    getFilename: (node) => node.dataset.id
}

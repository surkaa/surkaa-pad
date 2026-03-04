import {Extension, MenuButton} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils";

export const ImageExtension: Extension = {
    name: "image",

    match: (node) => node.nodeName === 'IMG',

    getMark: (filename) => `[[IMG:${filename}]]`,

    hasMark: (source, filename) => new RegExp(`\\[\\[IMG:${filename}(?:\\|[^\\]]*)?\]\\]`).test(source),

    toHtml: (source, ctx) => source.replace(/\[\[IMG:([^\]|]+)(?:\|([^\]]+))?]]/g, (_, filename, configStr) => {
        const url = ctx.getAttachmentUrl(filename) || resolveMediaAttachmentUrl('image', ctx.getDiaryId(), filename);
        const img = document.createElement('img');
        img.src = url;
        img.dataset.id = filename;
        // 解析配置项
        if (configStr) {
            const params = new URLSearchParams(configStr);
            if (params.get('size') === 'small') {
                img.dataset.size = 'small'; // 使用 data-size 属性标记小图模式
            }
        }

        return img.outerHTML;
    }),

    serialize: (n: HTMLElement) => {
        const node = n as HTMLImageElement;
        const filename = node.dataset.id;
        if (!filename) return '';

        const params = new URLSearchParams();
        if (node.dataset.size === 'small') {
            params.append('size', 'small');
        }

        const configStr = params.toString();
        return configStr ? `[[IMG:${filename}|${configStr}]]` : `[[IMG:${filename}]]`;
    },

    onClick: (_e, node, ctx) => {
        const filename = (node as HTMLImageElement).dataset.id;
        if (!filename) {
            console.error(`无法打开附件，因为没有找到 data-id 属性`);
            return;
        }
        const attachment = ctx.getAttachment(filename);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}`);
            return;
        }
        ctx.gotoPreview('image', ctx.getDiaryId(), attachment.filename);
    },

    // 右键菜单实现
    onContextmenu: (_e, node, _ctx): MenuButton[] => {
        const imgNode = node as HTMLImageElement;
        const isSmall = imgNode.dataset.size === 'small';

        return [{
            label: isSmall ? '大图模式' : '小图模式',
            action: (targetEl) => {
                const target = targetEl as HTMLImageElement;
                if (isSmall) {
                    delete target.dataset.size; // 移除属性，恢复默认
                } else {
                    target.dataset.size = 'small'; // 设置属性
                }
            }
        }];
    }
}

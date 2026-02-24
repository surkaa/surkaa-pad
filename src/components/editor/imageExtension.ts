import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";

export const ImageExtension: Extension = {
    name: "image",

    match: (node) => node.nodeName === 'IMG',

    getMark: (filename) => `[[IMG:${filename}]]`,

    toHtml: (source, ctx) => source.replace(/\[\[IMG:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, configStr) => {
        let sizeAttr = '';

        // 如果存在配置项(竖线后面的内容)，则使用 URLSearchParams 解析
        if (configStr) {
            const params = new URLSearchParams(configStr);
            const size = params.get('size');
            if (size === 'small') {
                sizeAttr += 'data-size="small"';
            }
        }

        const diaryId = ctx.getDiaryId();
        if (!diaryId.length) {
            console.error(`无法解析图片 ${filename}，因为没有找到日记 ID`);
            return '';
        }
        const attachment = ctx.getAttachment(filename, `[[IMG:${filename}]]`);

        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }

        const src = resolveMediaAttachmentUrl('image', diaryId, attachment.filename);

        return `<img src="${src}" data-id="${filename}" ${sizeAttr} alt="image" />`;
    }),

    serialize: (node: HTMLElement) => {
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
        const attachment = ctx.getAttachment(filename, `[[IMG:${filename}]]`);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}`);
            return;
        }
        ctx.gotoPreview('image', ctx.getDiaryId(), attachment.filename);
    }
}

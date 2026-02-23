import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";
import {commands} from "../../bindings.ts";

export const ImageExtension: Extension = {
    name: "image",

    style: `
        img[data-id] {
          padding: 5px;
          cursor: pointer;
          min-height: 50px;
          transition: width 0.3s ease;
          width: auto;
          max-width: 100%;
        }
        img[data-id]:hover {
          box-shadow: 0 0 0 3px rgba(64, 158, 255, 0.5);
        }
        img[data-size="small"] {
          width: 33% !important;
          display: inline-block;
        }
    `,

    match: (node) => node.nodeName === 'IMG',

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
        const attachment = ctx.getAttachment(filename);

        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }

        const src = resolveMediaAttachmentUrl('image', diaryId, attachment.filename);

        return `<img src="${src}" data-id="${filename}" ${sizeAttr} alt="image" />`;
    }),

    toSource: html => html.replace(/<img[^>]*data-id="([^"]*)"[^>]*>/gi, (match, filename) => {
        const params = new URLSearchParams();

        // 检查 HTML 字符串中是否包含状态属性
        if (match.includes('data-size="small"')) {
            params.append('size', 'small');
        }

        const configStr = params.toString();

        // 如果有配置项，就拼接竖线；如果没有，就返回最纯净的格式
        if (configStr) {
            return `[[IMG:${filename}|${configStr}]]`;
        } else {
            return `[[IMG:${filename}]]`;
        }
    }),

    onDeleted: async (node, ctx) => {
        const diaryId = ctx.getDiaryId();
        if (!diaryId) {
            console.error(`无法删除附件，因为没有找到日记 ID`);
            return;
        }
        const filename = (node as HTMLImageElement).dataset.id;
        if (!filename) {
            console.error(`无法删除附件，因为没有找到 data-id 属性`);
            return;
        }
        await commands.cmdDeleteAttachment(diaryId, filename);
        console.log('已删除附件：', filename);
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
    }
}

import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils";

export const VideoExtension: Extension = {
    name: "video",

    match: (node) => node.nodeName === 'VIDEO',

    getMark: (filename) => `[[VID:${filename}]]`,

    toHtml: (source, ctx) => source.replace(/\[\[VID:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const diaryId = ctx.getDiaryId();
        const attachment = ctx.getAttachment(filename);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }
        const src = ctx.getAttachmentUrl(filename) || resolveMediaAttachmentUrl('video', diaryId, filename);
        return `<video controls src="${src}" data-id="${filename}"></video>`;
    }),

    serialize: (node: HTMLElement) => {
        const filename = node.dataset.id;
        return filename ? `[[VID:${filename}]]` : '';
    },

    onClick: (_e, node, _ctx) => {
        console.log('点击了视频：', node);
    },

    isEncrypted: (node, ctx) => {
        const filename = (node as HTMLVideoElement).dataset.id;
        if (!filename) return false;
        const attachment = ctx.getAttachment(filename);
        return attachment ? attachment.encrypted : false;
    },

    getFilename: (node) => (node as HTMLVideoElement).dataset.id
}

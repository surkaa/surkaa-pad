import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";

export const VideoExtension: Extension = {
    name: "video",

    style: `
        video[data-id] {
            max-width: 100%;
            border-radius: 8px;
            margin: 10px 0;
            background: #000;
        }
    `,

    match: (node) => node.nodeName === 'VIDEO',

    toHtml: (source, ctx) => source.replace(/\[\[VID:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const diaryId = ctx.getDiaryId();
        const attachment = ctx.getAttachment(filename);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }
        const src = resolveMediaAttachmentUrl(diaryId, attachment);
        return `<video controls src="${src}" data-id="${filename}" />`;
    }),

    toSource: (html) => html.replace(/<video[^>]*data-id="([^"]*)"[^>]*>/gi, (_match, filename) => `[[VID:${filename}]]`),
}

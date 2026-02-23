import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";

export const AudioExtension: Extension = {
    name: "audio",

    style: `
        audio[data-id] {
            width: 100%;
            margin: 10px 0;
        }
    `,

    match: (node) => node.nodeName === 'AUDIO',

    toHtml: (source, ctx) => source.replace(/\[\[AUD:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const diaryId = ctx.getDiaryId();
        const attachment = ctx.getAttachment(filename);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }
        const src = resolveMediaAttachmentUrl(diaryId, attachment);
        return `<audio controls src="${src}" data-id="${filename}" />`;
    }),

    toSource: (html) => html.replace(/<audio[^>]*data-id="([^"]*)"[^>]*>/gi, (_match, filename) => `[[AUD:${filename}]]`),
}

import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";

export const AudioExtension: Extension = {
    name: "audio",

    match: (node) => node.nodeName === 'AUDIO',

    getMark: (filename) => `[[AUD:${filename}]]`,

    toHtml: (source, ctx) => source.replace(/\[\[AUD:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const diaryId = ctx.getDiaryId();
        const attachment = ctx.getAttachment(filename, `[[AUD:${filename}]]`);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }
        const src = resolveMediaAttachmentUrl('audio', diaryId, attachment.filename);
        return `<audio controls src="${src}" data-id="${filename}"></audio>`;
    }),

    toSource: (html) => html.replace(/<video[^>]*data-id="([^"]*)"[^>]*><\/video>/gi, (_match, filename) => `[[AUD:${filename}]]`),

    onClick: (_e, node, _ctx) => {
        console.log('点击了音频：', node);
    }
}

import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";
import {commands} from "../../bindings.ts";

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
        const src = resolveMediaAttachmentUrl('audio', diaryId, attachment.filename);
        return `<audio controls src="${src}" data-id="${filename}" />`;
    }),

    toSource: (html) => html.replace(/<audio[^>]*data-id="([^"]*)"[^>]*>/gi, (_match, filename) => `[[AUD:${filename}]]`),

    onDeleted: async (node, ctx) => {
        const diaryId = ctx.getDiaryId();
        if (!diaryId) {
            console.error(`无法删除附件，因为没有找到日记 ID`);
            return;
        }
        const filename = (node as HTMLAudioElement).dataset.id;
        if (!filename) {
            console.error(`无法删除附件，因为没有找到 data-id 属性`);
            return;
        }
        await commands.cmdDeleteAttachment(diaryId, filename);
        console.log('已删除附件：', filename);
    },

    onClick: (_e, node, _ctx) => {
        console.log('点击了音频：', node);
    }
}

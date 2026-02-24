import {Extension} from "./extension.ts";
import {resolveMediaAttachmentUrl} from "../../utils/resolveMediaAttachmentUrl.ts";
import {commands} from "../../bindings.ts";

export const VideoExtension: Extension = {
    name: "video",

    match: (node) => node.nodeName === 'VIDEO',

    toHtml: (source, ctx) => source.replace(/\[\[VID:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const diaryId = ctx.getDiaryId();
        const attachment = ctx.getAttachment(filename, `[[VID:${filename}]]`);
        if (!attachment) {
            console.error(`没有找到附件 ${filename}, 已自动移除`);
            return '';
        }
        const src = resolveMediaAttachmentUrl('video', diaryId, attachment.filename);
        return `<video controls src="${src}" data-id="${filename}" />`;
    }),

    toSource: (html) => html.replace(/<video[^>]*data-id="([^"]*)"[^>]*>/gi, (_match, filename) => `[[VID:${filename}]]`),

    onDeleted: async (node, ctx) => {
        const diaryId = ctx.getDiaryId();
        if (!diaryId) {
            console.error(`无法删除附件，因为没有找到日记 ID`);
            return;
        }
        const filename = (node as HTMLVideoElement).dataset.id;
        if (!filename) {
            console.error(`无法删除附件，因为没有找到 data-id 属性`);
            return;
        }
        await commands.cmdDeleteAttachment(diaryId, filename);
        console.log('已删除附件：', filename);
    },

    onClick: (_e, node, _ctx) => {
        console.log('点击了视频：', node);
    }
}

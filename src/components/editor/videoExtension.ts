import {Extension} from "./extension.ts";

export const VideoExtension: Extension<HTMLVideoElement> = {
    name: "video",

    match: (node) => node.nodeName === 'VIDEO',

    getMark: (filename) => `[[VID:${filename}]]`,

    toHtml: (source, ctx) => source.replace(/\[\[VID:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const src = ctx.getAttachmentUrl(filename);
        if (!src) {
            console.error(`无法找到视频附件：${filename}`);
            return '';
        }
        return `<video controls src="${src}" data-id="${filename}"></video>`;
    }),

    serialize: (node: HTMLElement) => {
        const filename = node.dataset.id;
        return filename ? `[[VID:${filename}]]` : '';
    },

    onClick: (_e, node, _ctx) => {
        console.log('点击了视频：', node);
    },

    getFilename: (node) => node.dataset.id
}

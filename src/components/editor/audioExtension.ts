import {Extension} from "./extension.ts";

export const AudioExtension: Extension<HTMLAudioElement> = {
    name: "audio",

    match: (node) => node.nodeName === 'AUDIO',

    getMark: (filename) => `[[AUD:${filename}]]`,

    toHtml: (source, ctx) => source.replace(/\[\[AUD:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const src = ctx.getAttachmentUrl(filename);
        if (!src) {
            console.error(`无法获取附件 ${filename} 的URL, 已自动移除`);
            return '';
        }
        return `<audio controls src="${src}" data-id="${filename}"></audio>`;
    }),

    serialize: (node: HTMLElement) => {
        const filename = node.dataset.id;
        return filename ? `[[AUD:${filename}]]` : '';
    },

    onClick: (_e, node, _ctx) => {
        console.log('点击了音频：', node);
    },

    getFilename: (node) => node.dataset.id
}

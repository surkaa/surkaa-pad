import {Extension, ExtensionContext} from "./extension.ts";
import {getFilename} from "./utils.ts";

function match(node: HTMLAudioElement) {
    return node.nodeName === 'AUDIO';
}

function getMark(filename: string) {
    return `[[AUD:${filename}]]`;
}

function toHtml(source: string, ctx: ExtensionContext) {
    return source.replace(/\[\[AUD:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const src = ctx.getAttachmentUrl(filename);
        if (!src) {
            console.error(`无法获取附件 ${filename} 的URL, 已自动移除`);
            return '';
        }
        return `<audio controls src="${src}" data-id="${filename}"></audio>`;
    });
}

function serialize(node: HTMLElement) {
    const filename = getFilename(node);
    return filename ? `[[AUD:${filename}]]` : '';
}

export const AudioExtension: Extension<HTMLAudioElement> = {
    name: "audio",
    match, getMark, toHtml, serialize, getFilename
}

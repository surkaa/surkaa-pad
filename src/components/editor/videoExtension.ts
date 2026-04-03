import {Extension, ExtensionContext} from "./extension.ts";
import {getFilename} from "./utils.ts";

function match(node: Node) {
    return node.nodeName === 'VIDEO' && (node as HTMLElement).hasAttribute('data-id');
}

function getMark(filename: string) {
    return `[[VID:${filename}]]`;
}

function toHtml(source: string, ctx: ExtensionContext) {
    return source.replace(/\[\[VID:([^|\]]+)(?:\|([^]]*))?]]/gi, (_match, filename, _configStr) => {
        const src = ctx.getAttachmentUrl(filename);
        if (!src) {
            console.error(`无法找到视频附件：${filename}`);
            return '';
        }
        return `<video controls src="${src}" data-id="${filename}"></video>`;
    });
}

function serialize(node: HTMLElement) {
    const filename = getFilename(node);
    return filename ? `[[VID:${filename}]]` : '';
}

function onClick(_e: MouseEvent, node: HTMLVideoElement, _ctx: ExtensionContext) {
    if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
    }
    console.log('点击了视频：', node);
}

export const VideoExtension: Extension<HTMLVideoElement> = {
    name: "video",
    match, getMark, toHtml, serialize, onClick, getFilename
}

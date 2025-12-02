import {invoke} from "@tauri-apps/api/core";
import {AttachmentMeta} from "../types";

// 定义图片标记的正则格式： <<IMG:文件名>>
const IMG_MARKER_REGEX = /<<IMG:(.+?)>>/g;

/**
 * 将后端返回的纯文本转换为带图片的 HTML
 * @param content 原始文本
 * @param diaryId 日记ID
 * @param attachments 附件列表 (从 Manifest 中获取，用于查找 nonce)
 */
export async function parseTextToHtml(
    content: string,
    diaryId: string,
    attachments: AttachmentMeta[]
): Promise<string> {
    if (!content) return "";

    const matches = [...content.matchAll(IMG_MARKER_REGEX)];

    // 没有图片标记，直接返回原文
    if (matches.length === 0) return content;

    let htmlContent = content;

    // 并行下载所有图片
    const tasks = matches.map(async (match) => {
        const fullMarker = match[0]; // <<IMG:abc.png>>
        const filename = match[1];   // abc.png

        // 找到对应的附件信息以获取 nonce
        const attachment = attachments.find(a => a.filename === filename);
        if (!attachment) {
            return {marker: fullMarker, html: `<div class="img-error">[图片丢失: ${filename}]</div>`};
        }

        try {
            // 调用后端下载接口
            const bytes = await invoke<number[]>("download_attachment", {
                uuid: diaryId,
                filename: filename,
                nonce: attachment.nonce
            });

            // 生成 Blob URL
            const blob = new Blob([new Uint8Array(bytes)], {type: attachment.mimetype || 'image/png'});
            const url = URL.createObjectURL(blob);

            // 生成 img 标签，注意加上 data-filename 方便保存时还原
            // style="display:block; width:100%" 满足你占满宽度的需求
            const imgTag = `<img src="${url}" data-filename="${filename}" class="diary-img" alt="${attachment.filename}"/>`;
            return {marker: fullMarker, html: imgTag};
        } catch (e) {
            console.error(`加载图片失败: ${filename}`, e);
            return {marker: fullMarker, html: `<div class="img-error">[加载失败: ${filename}]</div>`};
        }
    });

    const results = await Promise.all(tasks);

    // 替换文本中的标记
    results.forEach(item => {
        htmlContent = htmlContent.replace(item.marker, item.html);
    });

    return htmlContent;
}

/**
 * 将 HTML 内容还原为纯文本格式以便保存
 * @param htmlElement 编辑器的 DOM 节点
 */
export function parseHtmlToText(htmlElement: HTMLElement): string {
    // 不能简单用 innerText，因为需要把 img 标签变回 marker
    // 使用 Clone 节点的方法来处理，不影响界面
    const clone = htmlElement.cloneNode(true) as HTMLElement;

    // 找到所有的 img 标签
    const images = clone.querySelectorAll('img.diary-img');
    images.forEach(img => {
        const filename = img.getAttribute('data-filename');
        if (filename) {
            const textNode = document.createTextNode(`<<IMG:${filename}>>`);
            img.parentNode?.replaceChild(textNode, img);
        }
    });

    return clone.innerText;
}
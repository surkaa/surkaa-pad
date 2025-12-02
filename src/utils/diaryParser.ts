import {invoke} from "@tauri-apps/api/core";
import {AttachmentMeta} from "../types";

// 统一的媒体标记正则格式： <<TAG:文件名>>
const MEDIA_MARKER_REGEX = /<<(IMG|VID|AUD):(.+?)>>/g;
const TAG_MAP = {
    'IMG': 'img',
    'VID': 'video',
    'AUD': 'audio'
};

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

    const matches = [...content.matchAll(MEDIA_MARKER_REGEX)];

    // 没有标记，直接返回原文
    if (matches.length === 0) return content;

    let htmlContent = content;

    // 并行下载所有图片
    const tasks = matches.map(async (match) => {
        const fullMarker = match[0];    // <<IMG:abc.png>>
        const tagType = match[1];       // IMG, VID, or AUD
        const filename = match[2];      // abc.png

        // 找到对应的附件信息以获取 nonce 用于下载时解密
        const attachment = attachments.find(a => a.filename === filename);
        if (!attachment) {
            return {marker: fullMarker, html: `<div class="media-error">[媒体文件丢失: ${filename}]</div>`};
        }

        try {
            // 调用后端下载接口
            const bytes = await invoke<number[]>("download_attachment", {
                uuid: diaryId,
                filename: filename,
                nonce: attachment.nonce
            });

            // 生成 Blob URL
            const blob = new Blob([new Uint8Array(bytes)], {
                type: attachment.mimetype || 'application/octet-stream'
            });
            const url = URL.createObjectURL(blob);

            const elementTag = TAG_MAP[tagType as keyof typeof TAG_MAP];

            // 动态创建媒体元素
            let mediaHtml: string;
            if (elementTag === 'img') {
                mediaHtml = `<img src="${url}" data-filename="${filename}" class="diary-media diary-img" alt="${attachment.filename}"/>`;
            } else if (elementTag === 'video' || elementTag === 'audio') {
                // 视频和音频需要 controls 属性来显示播放器控件
                const additionalAttrs = elementTag === 'video' ? 'style="display:block; width:100%"' : '';
                mediaHtml = `<${elementTag} controls src="${url}" data-filename="${filename}" class="diary-media ${elementTag}" ${additionalAttrs}></${elementTag}>`;
            } else {
                mediaHtml = `[未知媒体类型: ${filename}]`;
            }

            return {marker: fullMarker, html: mediaHtml};
        } catch (e) {
            console.error(`加载媒体文件失败: ${filename}`, e);
            return {marker: fullMarker, html: `<div class="media-error">[加载失败: ${filename}]</div>`};
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

    const mediaElements = clone.querySelectorAll('.diary-media');
    mediaElements.forEach(media => {
        const filename = media.getAttribute('data-filename');
        const tagName = media.tagName.toUpperCase(); // IMG, VIDEO, AUDIO

        if (filename) {
            let tagPrefix = '';
            if (tagName === 'IMG') tagPrefix = 'IMG';
            else if (tagName === 'VIDEO') tagPrefix = 'VID';
            else if (tagName === 'AUDIO') tagPrefix = 'AUD';

            if (tagPrefix) {
                const textNode = document.createTextNode(`<<${tagPrefix}:${filename}>>`);
                // 用 TextNode 替换整个媒体元素
                media.parentNode?.replaceChild(textNode, media);
            }
        }
    });

    // --- 2. 换行符修正 (新增逻辑) ---

    let htmlString = clone.innerHTML;

    // a) 将 <br> 标签替换为单个换行符 \n (通常出现在段落内的强制换行)
    htmlString = htmlString.replace(/<br\s*\/?>/gi, '\n');

    // b) 将 <div> 和 <p> 的**开头**标签替换为两个换行符 \n\n
    // 这是解决段落丢失的核心。它标志着新段落的开始。
    htmlString = htmlString.replace(/<(?:p|div)\s*[^>]*>/gi, '\n');

    // c) 移除所有剩余的 HTML 标签（包括关闭标签 </div>, </p> 和所有行内标签）
    // 此时，段落分隔符 \n\n 已经被保留。
    htmlString = htmlString.replace(/<[^>]+>/g, '');

    // d) 清理冗余的换行符
    // 压缩三个或更多连续的换行符到 \n\n (标准段落分隔)，这会处理多个空行或连续的空块。
    // /g 确保全局替换，/i 确保不区分大小写。
    htmlString = htmlString.replace(/(\n\s*){3,}/g, '\n');

    // 最终返回清理后的字符串，并去除首尾可能存在的空白或换行符
    return htmlString.trim();
}
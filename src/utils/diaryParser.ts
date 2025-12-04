import {invoke} from "@tauri-apps/api/core";
import {AttachmentMeta} from "../types";
import {listen} from "@tauri-apps/api/event";
import {open, remove} from "@tauri-apps/plugin-fs";

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

        const elementTag = TAG_MAP[tagType as keyof typeof TAG_MAP];

        // 找到对应的附件信息以获取 nonce 用于下载时解密
        const attachment = attachments.find(a => a.filename === filename);
        if (!attachment) {
            return {marker: fullMarker, html: `<div class="media-error">[媒体文件丢失: ${filename}]</div>`};
        }

        try {
            // 后端download_attachment不返回任何东西，
            // 而是启动一个后台任务，下载完成后会emit前端
            // 所以这里直接用一个特定的URL格式，前端接收到下载完成的事件后替换
            const randomId = Math.random().toString(36).substring(2, 10);

            await invoke("download_attachment", {
                uuid: diaryId,
                filename: filename,
                nonce: attachment.nonce,
                eid: randomId,
            });

            let unlistedFn = await listen(`attachment_downloaded_${randomId}`, async (event) => {
                const payload = event.payload as { eid: string, tempPath: string };
                console.log("收到附件下载完成事件", payload.eid, payload.tempPath);
                // 创建数据URL
                const fileHandle = await open(payload.tempPath, {
                    read: true,
                });
                const stat = await fileHandle.stat();
                const buffer = new ArrayBuffer(stat.size);
                await fileHandle.read(new Uint8Array(buffer));
                await fileHandle.close();
                const blob = new Blob([buffer], {type: attachment.mimetype || 'application/octet-stream'});
                const dataUrl = URL.createObjectURL(blob);
                if (payload.eid === randomId) {
                    const mediaElement = document.getElementById(randomId) as HTMLImageElement | HTMLVideoElement | HTMLAudioElement | null;
                    if (mediaElement) {
                        if (elementTag === 'img') {
                            (mediaElement as HTMLImageElement).src = dataUrl;
                        } else if (elementTag === 'video' || elementTag === 'audio') {
                            (mediaElement as HTMLVideoElement | HTMLAudioElement).src = dataUrl;
                        }
                    }
                }
                // 取消监听
                if (unlistedFn) {
                    unlistedFn();
                }
                // 删除临时文件
                await remove(payload.tempPath);
            });

            // 动态创建媒体元素
            let mediaHtml: string;
            if (elementTag === 'img') {
                mediaHtml = `<img id="${randomId}" data-filename="${filename}" class="diary-media diary-img" alt="${attachment.filename}"/>`;
            } else if (elementTag === 'video' || elementTag === 'audio') {
                // 视频和音频需要 controls 属性来显示播放器控件
                const additionalAttrs = elementTag === 'video' ? 'style="display:block; width:100%"' : '';
                mediaHtml = `<${elementTag} controls id="${randomId}" data-filename="${filename}" class="diary-media ${elementTag}" ${additionalAttrs}></${elementTag}>`;
            } else {
                mediaHtml = `[未知媒体类型: ${filename}]`;
            }

            return {marker: fullMarker, html: mediaHtml};
        } catch (e) {
            console.error(`加载媒体文件失败: ${filename}`, e);
            return {
                marker: fullMarker,
                html: `<div class="media-error" data-tag="${elementTag}" data-filename="${attachment.filename}">[加载失败: ${filename}]</div>`
            };
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

    // 选择.diary-media和.media-error元素进行替换
    const mediaElements = clone.querySelectorAll('.diary-media, .media-error');
    mediaElements.forEach(media => {
        const filename = media.getAttribute('data-filename');
        const tagName = media.tagName.toUpperCase(); // IMG, VIDEO, AUDIO

        if (filename) {
            let tagPrefix = '';
            if (tagName === 'IMG') tagPrefix = 'IMG';
            else if (tagName === 'VIDEO') tagPrefix = 'VID';
            else if (tagName === 'AUDIO') tagPrefix = 'AUD';
            else if (media.classList.contains('media-error')) {
                const dataTag = media.getAttribute('data-tag');
                if (dataTag === 'img') tagPrefix = 'IMG';
                else if (dataTag === 'video') tagPrefix = 'VID';
                else if (dataTag === 'audio') tagPrefix = 'AUD';
            }

            if (tagPrefix) {
                const textNode = document.createTextNode(`<<${tagPrefix}:${filename}>>`);
                // 用 TextNode 替换整个媒体元素
                media.parentNode?.replaceChild(textNode, media);
            }
        }
    });

    let htmlString = clone.innerHTML;
    htmlString = htmlString.replace(/<br\s*\/?>/gi, '\n');
    htmlString = htmlString.replace(/<(?:p|div)\s*[^>]*>/gi, '\n');
    htmlString = htmlString.replace(/<[^>]+>/g, '');
    htmlString = htmlString.replace(/(\n\s*){3,}/g, '\n');
    // 把`&lt;`和`&gt;`还原
    htmlString = htmlString.replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>');
    return htmlString.trim();
}
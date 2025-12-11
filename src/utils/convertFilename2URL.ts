import {Channel, invoke} from "@tauri-apps/api/core";
import {open, remove} from "@tauri-apps/plugin-fs";
import {DownloadAttachmentEvent} from "../types";

type NullableFn = (() => void) | null;

const cancelMap: Map<string, NullableFn> = new Map();

/**
 * 将日记中的附件文件名转成可用的 URL 地址
 * @param uuid          日记 UUID
 * @param nonce         附件解密用的 nonce
 * @param eid           attachment media element id
 * @param mimetype      附件 mimetype
 * @param filename      附件文件名
 * @param urlCallback   转化成 URL 后的回调函数
 * @returns             取消下载和监听的函数
 */
export function convertFilename2URL(
    uuid: string,
    nonce: number[],
    eid: string,
    mimetype: string,
    filename: string,
    urlCallback: (url: string) => void
): () => void {
    const cancelFn = () => {
        // 取消监听事件
        const record = cancelMap.get(eid);
        if (!record) return;
        record();
        cancelMap.delete(eid);
    };

    const cancelDownloadFn = () => {
        invoke("cancel_download_attachment", {eid}).then(bool => {
            console.log(`[ConvertFilename2URL] 取消下载任务 ${eid}: ${bool}`);
        });
    };

    const onEvent = new Channel<DownloadAttachmentEvent>();
    onEvent.onmessage = async msg => {
        console.log('[ConvertFilename2URL] onmessage', msg);
        switch (msg.event) {
            case "started":
                console.log(`[ConvertFilename2URL] 下载附件 ${eid} 开始，大小 ${msg.data.totalSize} 字节`);
                // 添加取消下载函数到映射
                cancelMap.set(eid, cancelDownloadFn);
                break;
            case "downloadProgress":
                console.log(`[ConvertFilename2URL] 下载附件 ${eid} 进度：${msg.data.downloaded} 字节`);
                break;
            case "decrypting":
                console.log(`[ConvertFilename2URL] 附件 ${eid} 开始解密`);
                break;
            case "decrypted":
                console.log(`[ConvertFilename2URL] 附件 ${eid} 已解密，大小 ${msg.data.decryptedSize} 字节`);
                break;
            case "completed":
                console.log(`[ConvertFilename2URL] 附件 ${eid} 下载解密完成，文件路径 ${msg.data.filePath}`);
                // 创建数据URL
                const fileHandle = await open(msg.data.filePath, {
                    read: true,
                });
                const stat = await fileHandle.stat();
                const buffer = new ArrayBuffer(stat.size);
                await fileHandle.read(new Uint8Array(buffer));
                await fileHandle.close();
                const blob = new Blob([buffer], {type: mimetype || 'application/octet-stream'});
                const dataUrl = URL.createObjectURL(blob);
                // 删除临时文件
                await remove(msg.data.filePath);
                urlCallback(dataUrl);
                break;
            case "error":
                console.error(`[ConvertFilename2URL] 附件 ${eid} 下载并解密过程中出错：${msg.data.message}`);
                break;
            default:
                console.warn(`[ConvertFilename2URL] 未知的下载附件事件类型: ${(msg as any).event}`);
        }
    }

    invoke("download_attachment", {uuid, nonce, eid, filename, onEvent}).then(() => {
        console.log(`[ConvertFilename2URL] 已调用下载附件接口，eid: ${eid}`);
    });

    return cancelFn;
}
import {Channel, convertFileSrc, invoke} from "@tauri-apps/api/core";
import {DownloadAttachmentEvent} from "../types";

const cacheFileMap: Map<string, string> = new Map();


/**
 * 通过 eid 读取缓存的文件并转成 URL
 * @param eid      element id
 * @returns         数据 URL
 */
export function readCacheFile2UrlByEid(eid: string) {
    const filePath = cacheFileMap.get(eid);
    if (!filePath) {
        throw new Error(`[readCacheFile2URL] 未找到缓存的文件路径，eid: ${eid}`);
    }
    console.log('readCacheFile2URL', eid, filePath);
    return convertFileSrc(filePath);
}

/**
 * 将日记中的附件文件名转成可用的 URL 地址
 * @param uuid          日记 UUID
 * @param nonce         附件解密用的 nonce
 * @param eid           attachment media element id
 * @param filename      附件文件名
 * @param emit          更新状态的回调函数
 * @param urlCallback   转化成 URL 后的回调函数
 * @returns             取消下载和监听的函数
 */
export function convertFilename2URL(
    uuid: string,
    nonce: number[],
    eid: string,
    filename: string,
    emit: (type: DownloadAttachmentEvent['event'], msg: string) => void,
    urlCallback: (url: string) => void
): () => void {
    let ct: string | "will_cancel" | null = null;
    const cancelFn = () => {
        if (ct) {
            cancelDownloadFn(ct);
        } else {
            ct = "will_cancel";
        }
    };

    const cancelDownloadFn = (cancelToken: string) => {
        invoke("cancel_task", {cancelToken}).then(bool => {
            console.log(`[ConvertFilename2URL] 取消下载任务 ${eid}: ${bool}`);
        });
    };

    const onEvent = new Channel<DownloadAttachmentEvent>();
    let totalMB = 0;
    let decryptedMB = 0;
    onEvent.onmessage = async msg => {
        // console.log('[ConvertFilename2URL] onmessage', msg);
        switch (msg.event) {
            case "started":
                totalMB = msg.data.totalSize >> 20;
                console.log(`[ConvertFilename2URL] 下载附件 ${eid} 开始，大小 ${totalMB}MB`);
                break;
            case "downloadProgress":
                const downloadedMB = msg.data.downloaded >> 20;
                emit("downloadProgress", `${downloadedMB}MB / ${totalMB}MB`);
                break;
            case "decrypting":
                console.log(`[ConvertFilename2URL] 附件 ${eid} 开始解密`);
                emit("decrypting", `解密中...请稍等`);
                break;
            case "decrypted":
                decryptedMB = msg.data.decryptedSize >> 20;
                console.log(`[ConvertFilename2URL] 附件 ${eid} 已解密，大小 ${decryptedMB}MB / ${totalMB}MB`);
                break;
            case "completed":
                console.log(`[ConvertFilename2URL] 附件 ${eid} 下载解密完成，文件路径 ${msg.data.filePath}`);
                cacheFileMap.set(eid, msg.data.filePath);
                urlCallback(readCacheFile2UrlByEid(eid));
                break;
            case "error":
                console.error(`[ConvertFilename2URL] 附件 ${eid} 下载并解密过程中出错：${msg.data.message}`);
                break;
            default:
                console.warn(`[ConvertFilename2URL] 未知的下载附件事件类型: ${(msg as any).event}`);
        }
    }

    invoke<string>("download_attachment", {uuid, nonce, eid, filename, onEvent}).then((cancelToken) => {
        console.log(`[ConvertFilename2URL] 已调用下载附件接口，eid: ${eid}, cancelToken: ${cancelToken}`);
        if (ct === "will_cancel") {
            cancelDownloadFn(cancelToken);
        } else {
            ct = cancelToken;
        }
    });

    return cancelFn;
}
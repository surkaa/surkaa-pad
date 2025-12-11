import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import {open, remove} from "@tauri-apps/plugin-fs";

type ListenEventType = {
    payload: { eid: string, tempPath: string }
}

type NullableFn = (() => void) | null;

const cancelMap: Map<string, {
    unlistenFn: NullableFn;
    cancelDownloadFn: NullableFn;
    url: string;
}> = new Map();

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
        if (record.unlistenFn) {
            record.unlistenFn();
        }
        // 取消下载任务
        if (record.cancelDownloadFn) {
            record.cancelDownloadFn();
        }
        cancelMap.delete(eid);
    };

    const cancelDownloadFn = () => {
        invoke("cancel_download_attachment", {eid}).then(bool => {
            console.log(`取消下载任务 ${eid}: ${bool}`);
        });
    };

    invoke("download_attachment", {
        uuid,
        nonce,
        eid,
        filename: filename,
    }).then(() => {
        // 请求下载成功，填充到 cancelMap
        const record = cancelMap.get(eid) || {
            unlistenFn: null,
            cancelDownloadFn,
            url: '',
        };
        cancelMap.set(eid, record);

        // 监听下载完成事件
        listen(`attachment_downloaded_${eid}`, async (event: ListenEventType) => {
            // 创建数据URL
            const fileHandle = await open(event.payload.tempPath, {
                read: true,
            });
            const stat = await fileHandle.stat();
            const buffer = new ArrayBuffer(stat.size);
            await fileHandle.read(new Uint8Array(buffer));
            await fileHandle.close();
            const blob = new Blob([buffer], {type: mimetype || 'application/octet-stream'});
            const dataUrl = URL.createObjectURL(blob);
            // 删除临时文件
            await remove(event.payload.tempPath);
            urlCallback(dataUrl);
        }).then(unlistenFn => {
            // 填充取消监听函数
            const record = cancelMap.get(eid);
            if (record) {
                record.unlistenFn = unlistenFn;
            } else {
                console.warn(`取消映射未找到 eid=${eid} 的记录`);
            }
        });
    });

    return cancelFn;
}
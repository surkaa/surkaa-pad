import {Channel} from "@tauri-apps/api/core";
import {AddAttachmentEvent, commands} from "../bindings.ts";
import {ref} from "vue";
import {open} from "@tauri-apps/plugin-dialog"

export function useMediaAction(diaryId: string) {
    const cancelTokens = ref<string[]>([]);

    const uploadAttachment = async (accessStr: string, mimetype: string, encrypted: boolean) => {
        const event = new Channel<AddAttachmentEvent>();
        let cancelToken = "";
        event.onmessage = (msg) => {
            switch (msg.event) {
                case "started":
                    console.log("开始上传");
                    break;
                case "progress":
                    console.log("百分制整数进度", msg.data);
                    break;
                case "completed":
                    console.log("上传完成，附件Meta", msg.data);
                    if (cancelToken) {
                        // 去掉cancelToken
                        const index = cancelTokens.value.indexOf(cancelToken);
                        if (index !== -1) {
                            cancelTokens.value.splice(index, 1);
                        }
                    }
                    break;
                case "error":
                    console.error("上传失败，错误信息", msg.data);
                    break;
            }
        };
        const res = await commands.cmdAddAttachment(event, diaryId, accessStr, mimetype, encrypted);
        if (res.status == "error") {
            console.log("调用 Rust 后端失败", res.error);
            return;
        }
        cancelToken = res.data;
        cancelTokens.value.push(res.data);
    }

    return {
        cancelTokens,
        insertPhoto: async () => {
            const accessStrArr = await open({
                multiple: true,
                pickerMode: 'image',
                filters: [{
                    name: 'Images',
                    extensions: ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp']
                }]
            });
            if (!accessStrArr) return;
            for (const accessStr of accessStrArr) {
                await uploadAttachment(accessStr, "image/*", true);
            }
        },
        takePhoto: async () => {
            // TODO
        },
        audioRecording: () => {
            // TODO
        },
        insertVideo: async () => {
            const accessStrArr = await open({
                multiple: true,
                pickerMode: 'video',
                filters: [{
                    name: 'Videos',
                    extensions: ['mp4', 'avi', 'mov', 'mkv', 'webm']
                }]
            });
            if (!accessStrArr) return;
            for (const accessStr of accessStrArr) {
                await uploadAttachment(accessStr, "video/*", true);
            }
        },
        takeVideo: () => {
            // TODO
        },
        insertFile: async () => {
            const accessStrArr = await open({multiple: true, pickerMode: 'document'});
            if (!accessStrArr) return;
            for (const accessStr in accessStrArr) {
                await uploadAttachment(accessStr, "document/*", true);
            }
        }
    };

}
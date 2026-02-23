import {Channel} from "@tauri-apps/api/core";
import {AddAttachmentEvent, AttachmentMeta, commands} from "../bindings.ts";
import {onUnmounted, Ref} from "vue";
import {open} from "@tauri-apps/plugin-dialog"
import {resolveMediaAttachmentUrl} from "../utils/resolveMediaAttachmentUrl.ts";
import {insertBlockNode} from "../utils/domUtils.ts";
import {useQuasar} from "quasar";

export function useMediaAction(diaryId: string, editorDomRef: Ref<HTMLElement | undefined>) {
    const $q = useQuasar();
    const cancelTokens: string[] = [];

    async function uploadAttachment(accessStr: string, mimetype: string, encrypted: boolean, completedCallback?: (meta: AttachmentMeta) => void) {
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
                    completedCallback && completedCallback(msg.data);
                    console.log("上传完成，附件Meta", msg.data);
                    if (cancelToken) {
                        // 去掉cancelToken
                        const index = cancelTokens.indexOf(cancelToken);
                        if (index !== -1) {
                            cancelTokens.splice(index, 1);
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
        cancelTokens.push(res.data);
    }

    onUnmounted(async () => {
        const results = await Promise.all(
            cancelTokens.map(token => commands.cmdCancelTask(token))
        );
        for (const result of results) {
            if (result.status === "error") {
                console.error("取消上传任务失败", result.error);
            }
        }
    });

    return {
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
                await uploadAttachment(accessStr, "image/*", true, (att) => {
                    // 立即渲染
                    const url = resolveMediaAttachmentUrl('image', diaryId, att.filename);
                    console.log('插入图片，URL:', url);
                    const img = document.createElement('img');
                    img.src = url;
                    img.dataset.id = att.filename;
                    if (editorDomRef.value) {
                        insertBlockNode(editorDomRef.value, img);
                    } else {
                        $q.notify({type: 'negative', message: 'EditorDOM节点未找到'});
                    }
                });
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
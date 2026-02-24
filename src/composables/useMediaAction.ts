import {Channel} from "@tauri-apps/api/core";
import {AddAttachmentEvent, AttachmentMeta, commands} from "../bindings.ts";
import {computed, onUnmounted, Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog"
import {resolveMediaAttachmentUrl} from "../utils/resolveMediaAttachmentUrl.ts";
import {insertBlockNode} from "../utils/domUtils.ts";
import {useQuasar} from "quasar";

export interface UploadTask {
    filename: string;
    progress: number;
    status: 'pending' | 'uploading' | 'completed' | 'error';
}

export function useMediaAction(diaryId: Ref<string>, editorDomRef: Ref<HTMLElement | undefined>, showPanel: Ref<boolean>) {
    const $q = useQuasar();
    const cancelTokens: string[] = [];

    // 进度管理状态
    const uploadTaskMap = ref<Record<string, UploadTask>>({});
    const showUploadDialog = ref(false);
    const uploadTasks = computed(() => Object.values(uploadTaskMap.value));
    const isUploading = computed(() => {
        if (uploadTasks.value.length === 0) return true;
        return uploadTasks.value.every(task => task.status === 'completed' || task.status === 'error');
    });

    async function uploadAttachment(
        accessStr: string,
        mimetype: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta) => void
    ) {
        // 从路径提取文件名用于占位显示
        const rawName = accessStr.split(/[\\/]/).pop() || "未知文件";
        const key = crypto.randomUUID();
        uploadTaskMap.value[key] = {filename: rawName, progress: 0, status: 'pending'};
        showUploadDialog.value = true;

        const event = new Channel<AddAttachmentEvent>();
        let cancelToken = "";
        event.onmessage = (msg) => {
            switch (msg.event) {
                case "started":
                    uploadTaskMap.value[key].status = 'uploading';
                    break;
                case "progress":
                    uploadTaskMap.value[key].progress = msg.data / 100;
                    break;
                case "completed":
                    uploadTaskMap.value[key].status = 'completed';
                    uploadTaskMap.value[key].progress = 1;
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
                    uploadTaskMap.value[key].status = 'error';
                    $q.notify({type: 'negative', message: `${uploadTaskMap.value[key].filename} 上传失败: ${msg.data}`});
                    break;
            }
        };
        const res = await commands.cmdAddAttachment(event, diaryId.value, accessStr, mimetype, encrypted);
        if (res.status == "error") {
            uploadTaskMap.value[key].status = 'error';
            console.error("调用 Rust 后端失败", res.error);
            return;
        }
        cancelToken = res.data;
        cancelTokens.push(res.data);
    }

    // 捕获光标
    const captureRange = (): Range | null => {
        const sel = window.getSelection();
        return sel && sel.rangeCount > 0 ? sel.getRangeAt(0).cloneRange() : null;
    };

    function beforeClick() {
        if (showPanel.value) {
            showPanel.value = false;
        }
        if (editorDomRef.value) {
            editorDomRef.value.focus();
        }
    }

    const genericBatchUpload = async (pickerMode: PickerMode, extensions: string[], mimetype: string, nodeType: 'img' | 'video' | 'audio') => {
        let currentRange = captureRange();
        beforeClick();
        const accessStrArr = await open({
            multiple: true,
            pickerMode: pickerMode,
            filters: [{name: pickerMode, extensions}]
        });
        if (!accessStrArr) return;

        // 重置上传列表
        uploadTaskMap.value = {};

        const uploads = accessStrArr.map(accessStr =>
            uploadAttachment(accessStr, mimetype, true, (att) => {
                const url = resolveMediaAttachmentUrl(nodeType === 'img' ? 'image' : 'video', diaryId.value, att.filename);
                const el = document.createElement(nodeType);
                if (nodeType !== 'img') (el as HTMLMediaElement).controls = true;
                (el as any).src = url;
                el.dataset.id = att.filename;

                if (editorDomRef.value) {
                    insertBlockNode(editorDomRef.value, el, currentRange);
                }
            })
        );
        await Promise.allSettled(uploads);
    };

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
        uploadTasks,
        showUploadDialog,
        isUploading,
        insertPhoto: () => genericBatchUpload('image', ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'], "image/*", 'img'),
        takePhoto: async () => {
            // TODO
        },
        audioRecording: () => {
            // TODO
        },
        insertAudio: () => genericBatchUpload('media', ['mp3', 'wav', 'ogg', 'flac', 'aac'], "audio/*", 'audio'),
        insertVideo: () => genericBatchUpload('video', ['mp4', 'avi', 'mov', 'mkv', 'webm'], "video/*", 'video'),
        takeVideo: () => {
            // TODO
        },
        insertFile: async () => {
            // TODO
        }
    };
}

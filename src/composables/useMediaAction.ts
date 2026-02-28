import {Channel} from "@tauri-apps/api/core";
import {AddAttachmentEvent, AttachmentMeta, commands} from "../bindings.ts";
import {computed, onUnmounted, Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog";
import {resolveMediaAttachmentUrl} from "../utils/resolveMediaAttachmentUrl.ts";
import {insertMediaNode, MediaType} from "../utils/domUtils.ts";
import {useQuasar} from "quasar";

export interface UploadTask {
    filename: string;
    progress: number;
    status: 'pending' | 'uploading' | 'completed' | 'error';
}

export function useMediaAction(diaryId: Ref<string>, editorDomRef: Ref<HTMLElement | undefined>, showPanel: Ref<boolean>) {
    const $q = useQuasar();
    const cancelTokens = new Set<string>();

    // 进度管理状态
    const uploadTaskMap = ref<Record<string, UploadTask>>({});
    const showUploadDialog = ref(false);
    const showAudioDrawer = ref(false);

    const uploadTasks = computed(() => Object.values(uploadTaskMap.value));
    const isUploading = computed(() => {
        if (uploadTasks.value.length === 0) return true;
        return uploadTasks.value.every(task => task.status === 'completed' || task.status === 'error');
    });

    function createUploadChannel(key: string, onSuccess?: (meta: AttachmentMeta) => void) {
        const event = new Channel<AddAttachmentEvent>();
        event.onmessage = (msg) => {
            const task = uploadTaskMap.value[key];
            if (!task) return;

            switch (msg.event) {
                case "started":
                    task.status = 'uploading';
                    break;
                case "progress":
                    task.progress = msg.data / 100;
                    break;
                case "completed":
                    task.status = 'completed';
                    task.progress = 1;
                    onSuccess?.(msg.data);
                    break;
                case "error":
                    task.status = 'error';
                    $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${msg.data}`});
                    break;
            }
        };
        return event;
    }

    function handleCommandResult(key: string, res: { status: "ok" | "error", data?: string, error?: string }) {
        if (res.status === "error") {
            uploadTaskMap.value[key].status = 'error';
            console.error("调用 Rust 后端失败:", res.error);
            return;
        }
        if (res.data) cancelTokens.add(res.data);
    }

    async function uploadAttachment(
        accessStr: string,
        mimetype: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta) => void
    ) {
        const rawName = accessStr.split(/[\\/]/).pop() || "未知文件";
        const key = crypto.randomUUID();
        uploadTaskMap.value[key] = {filename: rawName, progress: 0, status: 'pending'};
        showUploadDialog.value = true;

        const event = createUploadChannel(key, completedCallback);

        const res = await commands.cmdAddAttachment(event, diaryId.value, accessStr, mimetype, encrypted);
        handleCommandResult(key, res);
    }

    async function uploadMemoryAttachment(
        filename: string,
        bytes: Uint8Array,
        mimetype: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta) => void
    ) {
        const key = crypto.randomUUID();
        uploadTaskMap.value[key] = {filename, progress: 0, status: 'pending'};
        showUploadDialog.value = true;

        const event = createUploadChannel(key, completedCallback);
        // @ts-ignore
        const res = await commands.cmdAddAttachmentMemory(event, diaryId.value, bytes, mimetype, encrypted);
        handleCommandResult(key, res);
    }

    const captureRange = (): Range | null => {
        const sel = window.getSelection();
        return sel && sel.rangeCount > 0 ? sel.getRangeAt(0).cloneRange() : null;
    };

    function beforeClick() {
        if (!diaryId.value) {
            $q.notify({type: 'warning', message: '请先创建日记才能使用录音功能'});
            return true;
        }
        if (showPanel.value) showPanel.value = false;
        editorDomRef.value?.focus();
    }

    const genericBatchUpload = async (pickerMode: PickerMode, extensions: string[], mimetype: string, nodeType: MediaType) => {
        const currentRange = captureRange();
        if (beforeClick()) return;
        const accessStrArr = await open({
            multiple: true,
            pickerMode: pickerMode,
            filters: [{name: pickerMode, extensions}]
        });
        if (!accessStrArr) return;

        const uploads = accessStrArr.map(accessStr =>
            uploadAttachment(accessStr, mimetype, true, (att) => {
                // 原代码中图片视为 'image'，音视频可能共用 'video'，这里保持原有逻辑或按需修正
                const resolveType = nodeType === 'img' ? 'image' : 'video';
                const url = resolveMediaAttachmentUrl(resolveType, diaryId.value, att.filename);
                insertMediaNode(editorDomRef.value, nodeType, url, att.filename, currentRange);
            })
        );
        await Promise.allSettled(uploads);
    };

    onUnmounted(async () => {
        if (cancelTokens.size === 0) return;

        const cancelPromises = Array.from(cancelTokens).map(token => commands.cmdCancelTask(token));
        const results = await Promise.allSettled(cancelPromises);

        for (const result of results) {
            if (result.status === "rejected" || (result.status === "fulfilled" && (result.value as any)?.status === "error")) {
                console.error("取消上传任务失败:", result);
            }
        }
        cancelTokens.clear();
    });

    return {
        uploadTasks,
        showUploadDialog,
        isUploading,
        showAudioDrawer,
        handleAudioRecorded: async (mimetype: string, stream: ReadableStream<Uint8Array>) => {
            showAudioDrawer.value = false;
            const currentRange = captureRange();
            if (beforeClick()) return;

            const arrayBuffer = await new Response(stream).arrayBuffer();
            const uint8Array = new Uint8Array(arrayBuffer);
            const virtualName = `Audio_${new Date().toISOString().replace(/[:.]/g, '-')}.webm`;

            await uploadMemoryAttachment(virtualName, uint8Array, mimetype, true, (att) => {
                const url = resolveMediaAttachmentUrl('video', diaryId.value, att.filename);
                insertMediaNode(editorDomRef.value, 'audio', url, att.filename, currentRange);
            });
        },
        insertPhoto: () => genericBatchUpload('image', ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'], "image/*", 'img'),
        takePhoto: async () => {
            // TODO
        },
        audioRecording: () => {
            if (beforeClick()) return;
            showAudioDrawer.value = true;
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

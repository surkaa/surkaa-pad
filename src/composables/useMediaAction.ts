import {Channel} from "@tauri-apps/api/core";
import {AttachmentMeta, AttachmentProcessEvent, commands} from "../bindings.ts";
import {computed, onUnmounted, Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog";
import {formatBytes, insertFileNode, insertMediaNode, MediaType} from "../utils";
import {useQuasar} from "quasar";
import {v4 as uuidv4} from "uuid";
import {useDataStore} from "../stores/data.ts";
import {storeToRefs} from "pinia";

export interface UploadTask {
    filename: string;
    progress: number;
    status: 'pending' | 'uploading' | 'completed' | 'error';
}

type OnAttachmentProcessSuccess = (meta: AttachmentMeta, url: string) => void;

export function useMediaAction(
    diaryId: Ref<string>,
    editorDomRef: Ref<HTMLElement | undefined>,
    showPanel: Ref<boolean>,
    updateAttachmentUrl: (filename: string, url: string) => void
) {
    const $q = useQuasar();
    const dataStore = useDataStore();
    const {currentDiaryAttachmentUrlMap} = storeToRefs(dataStore);

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

    function createUploadChannel(key: string, onSuccess?: OnAttachmentProcessSuccess) {
        const event = new Channel<AttachmentProcessEvent>();
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
                    const [meta, url] = msg.data;
                    onSuccess?.(meta, url);
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
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta, url: string) => void
    ) {
        const rawName = accessStr.split(/[\\/]/).pop() || "未知文件";
        const key = uuidv4();
        uploadTaskMap.value[key] = {filename: rawName, progress: 0, status: 'pending'};

        const event = createUploadChannel(key, completedCallback);

        const res = await commands.cmdAddAttachment(event, diaryId.value, accessStr, encrypted);
        handleCommandResult(key, res);
    }

    async function uploadMemoryAttachment(
        filename: string,
        bytes: Uint8Array,
        mimetype: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta, url: string) => void
    ) {
        const key = uuidv4();
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
        // 清除旧任务
        uploadTaskMap.value = {};
        editorDomRef.value?.focus();
    }

    const genericBatchUpload = async (encrypted: boolean, extensions: string[], nodeType?: MediaType, pickerMode?: PickerMode) => {
        const currentRange = captureRange();
        if (beforeClick()) return;
        const accessStrArr = await open({
            multiple: true,
            pickerMode: pickerMode,
            filters: [{name: pickerMode || 'filter file', extensions}]
        });
        console.log('选中文件:', accessStrArr);
        if (!accessStrArr) return;

        const uploads = accessStrArr.map(accessStr =>
            uploadAttachment(accessStr, encrypted, (att, url) => {
                if (!nodeType) {
                    insertFileNode(editorDomRef.value, att.filename, formatBytes(att.size), currentRange);
                    return;
                }
                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                insertMediaNode(editorDomRef.value, nodeType, url, att.filename, currentRange);
            })
        );
        showUploadDialog.value = true;
        await Promise.allSettled(uploads);
    };

    async function toggleAttachmentEncryption(filename: string, encrypted: boolean) {
        return new Promise<void>((resolve, reject) => {
            if (!diaryId.value || !filename || !diaryId.value.trim() || !filename.trim()) {
                console.log(`无法获取日记ID或文件名，无法执行转换。diaryId: ${diaryId.value}, filename: ${filename}`);
                $q.notify({type: 'negative', message: '无法获取日记ID或文件名，无法执行转换'});
                reject(new Error('Invalid diary ID or filename'));
                return;
            }
            uploadTaskMap.value = {};
            editorDomRef.value?.focus();

            const key = uuidv4();
            uploadTaskMap.value[key] = {filename, progress: 0, status: 'pending'};

            const event = createUploadChannel(key, (meta, url) => {
                console.log('转换完成:', filename, meta.encrypted, url);
                dataStore.updateAttachment(diaryId.value, meta);
                updateAttachmentUrl(filename, url);
                resolve();
            });
            commands.cmdToggleAttachmentEncryption(
                event,
                diaryId.value,
                filename,
                encrypted
            ).then(cancelRes => {
                if (cancelRes.status === "error") {
                    $q.notify({type: 'negative', message: cancelRes.error});
                    reject(new Error(cancelRes.error));
                    return;
                } else {
                    showUploadDialog.value = true;
                    console.log('转换附件命令已发送，取消令牌:', cancelRes.data);
                }
            });
            console.log('发送转换附件命令，等待结果...');
        });
    }

    async function rotateAttachment(filename: string, rotation: number) {
        if (!diaryId.value || !filename || !diaryId.value.trim() || !filename.trim()) {
            console.log(`无法获取日记ID或文件名，无法执行旋转。diaryId: ${diaryId.value}, filename: ${filename}`);
            $q.notify({type: 'negative', message: '无法获取日记ID或文件名，无法执行旋转'});
            return;
        }
        if ([90, 180, -90].indexOf(rotation) === -1) {
            console.log(`无效的旋转角度: ${rotation}`);
            $q.notify({type: 'negative', message: '无效的旋转角度'});
            return;
        }
        uploadTaskMap.value = {};
        editorDomRef.value?.focus();
        const key = uuidv4();
        uploadTaskMap.value[key] = {filename, progress: 0, status: 'pending'};

        const event = createUploadChannel(key, (meta, url) => {
            console.log('旋转完成:', filename, url);
            dataStore.updateAttachment(diaryId.value, meta);
            updateAttachmentUrl(filename, url);
        });

        const res = await commands.cmdRotateImageAttachment(
            event,
            diaryId.value,
            filename,
            rotation
        );
        if (res.status === "error") {
            uploadTaskMap.value[key].status = 'error';
            $q.notify({type: 'negative', message: res.error});
            console.error('旋转图片失败:', res.error);
        } else {
            showUploadDialog.value = true;
            console.log('发送旋转图片命令，等待结果...');
        }
    }

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

            await uploadMemoryAttachment(virtualName, uint8Array, mimetype, true, (att, url) => {
                insertMediaNode(editorDomRef.value, 'audio', url, att.filename, currentRange);
            });
        },
        insertPhoto: () => genericBatchUpload(true, ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'], 'img', "image"),
        takePhoto: async () => {
            const currentRange = captureRange();
            if (beforeClick()) return;
            const key = uuidv4();
            uploadTaskMap.value[key] = {filename: 'take photo', progress: 0, status: 'pending'};
            const event = createUploadChannel(key, (meta, url) => {
                insertMediaNode(editorDomRef.value, 'img', url, meta.filename, currentRange);
            });
            const res = await commands.cmdAddImageAttachmentFromCamera(event, diaryId.value, true);
            handleCommandResult(key, res);
        },
        audioRecording: () => {
            if (beforeClick()) return;
            showAudioDrawer.value = true;
        },
        insertAudio: () => genericBatchUpload(false, ['mp3', 'wav', 'ogg', 'flac', 'aac'], 'audio'),
        insertVideo: () => genericBatchUpload(false, ['mp4', 'avi', 'mov', 'mkv', 'webm'], 'video', "video"),
        insertFile: async () => genericBatchUpload(true, ['doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'pdf', 'txt', 'zip', 'rar']),
        toggleAttachmentEncryption,
        rotateAttachment,
    };
}

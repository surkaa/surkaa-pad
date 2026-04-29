import {Channel} from "@tauri-apps/api/core";
import {AttachmentMeta, AttachmentProcessEvent} from "../bindings.ts";
import {computed, onUnmounted, Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog";
import {useQuasar} from "quasar";
import {v4 as uuidv4} from "uuid";
import {useDataStore} from "../stores/data.ts";
import {storeToRefs} from "pinia";
import TiptapEditor from "../components/TiptapEditor.vue";
import api from "../utils/api.ts";
import {formatError} from "../utils/formatError.ts";

export interface UploadTask {
    filename: string;
    progress: number;
    status: 'pending' | 'uploading' | 'completed' | 'error';
}

type OnAttachmentProcessSuccess = (meta: AttachmentMeta, url: string) => void;

const PHOTO_TYPES = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'];
const AUDIO_TYPES = ['mp3', 'wav', 'ogg', 'flac', 'aac'];
const VIDEO_TYPES = ['mp4', 'avi', 'mov', 'mkv', 'webm'];

// TODO 给那个弹窗增加取消功能、显示错误的功能，同时禁用页面返回避免直接取消。
export function useMediaAction(
    diaryId: Ref<string>,
    editorDomRef: Ref<HTMLElement | undefined>,
    showPanel: Ref<boolean>,
    editorContentRef: Ref<InstanceType<typeof TiptapEditor> | undefined>
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
                case "completedWithoutData":
                    task.status = 'completed';
                    task.progress = 1;
                    break;
                case "error":
                    task.status = 'error';
                    $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${msg.data}`});
                    break;
            }
        };
        return event;
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

        try {
            const res = await api.cmdAddAttachment(event, diaryId.value, accessStr, encrypted);
            cancelTokens.add(res);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            console.error("调用 Rust 后端失败:", e);
        }
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
        try {
            // @ts-ignore
            const res = await api.cmdAddAttachmentMemory(event, diaryId.value, bytes, mimetype, encrypted);
            cancelTokens.add(res);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            console.error("调用 Rust 后端失败:", e);
        }
    }

    function beforeClick() {
        if (!diaryId.value) {
            $q.notify({type: 'warning', message: '请先创建日记才能使用此功能'});
            return true;
        }
        if (showPanel.value) showPanel.value = false;
        // 清除旧任务
        uploadTaskMap.value = {};
        editorDomRef.value?.focus();
    }

    async function genericBatchUpload(encrypted: boolean, extensions?: string[], nodeType?: string, pickerMode?: PickerMode) {
        if (beforeClick()) return;
        const accessStrArr = await open({
            multiple: true,
            pickerMode: pickerMode,
            filters: extensions ? [{name: pickerMode || 'filter file', extensions}] : undefined
        });
        console.log('选中文件:', accessStrArr);
        if (!accessStrArr) return;

        const uploads = accessStrArr.map(accessStr =>
            uploadAttachment(accessStr, encrypted, (att, url) => {
                if (!editorContentRef.value) {
                    console.error('编辑器内容引用未定义，无法插入媒体节点');
                    return;
                }
                if (!nodeType) {
                    editorContentRef.value.insertFile(att.filename);
                    return;
                }
                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                if (nodeType === 'img') editorContentRef.value.insertImage(att.filename);
                else if (nodeType === 'video') editorContentRef.value.insertVideo(att.filename);
                else if (nodeType === 'audio') editorContentRef.value.insertAudio(att.filename);
            })
        );
        showUploadDialog.value = true;
        await Promise.allSettled(uploads);
    }

    async function performAttachmentOperation<Args extends any[]>(
        filename: string,
        operationName: string,
        apiCall: (event: Channel<AttachmentProcessEvent>, diaryId: string, filename: string, ...args: Args) => Promise<string>,
        ...apiArgs: Args
    ) {
        // 验证日记ID和文件名
        if (!diaryId.value || !filename || !diaryId.value.trim() || !filename.trim()) {
            console.log(`无法获取日记ID或文件名，无法执行${operationName}。diaryId: ${diaryId.value}, filename: ${filename}`);
            $q.notify({type: 'negative', message: `无法获取日记ID或文件名，无法执行${operationName}`});
            return;
        }

        uploadTaskMap.value = {};
        editorDomRef.value?.focus();

        const key = uuidv4();
        uploadTaskMap.value[key] = {filename, progress: 0, status: 'pending'};

        const event = createUploadChannel(key, (meta, url) => {
            console.log(`${operationName}完成:`, filename, meta.encrypted, url);
            dataStore.updateAttachment(diaryId.value, meta);
            if (!editorContentRef.value) {
                console.error('编辑器内容引用未定义，无法更新媒体链接');
                $q.notify({type: 'negative', message: '编辑器内容引用未定义，无法更新媒体链接'});
                return;
            }
            const res = editorContentRef.value.updateSrc(filename, url);
            if (!res) {
                console.warn('未找到对应的附件元素，无法更新链接:', filename);
            }
        });

        try {
            const cancelRes = await apiCall(event, diaryId.value, filename, ...apiArgs);
            showUploadDialog.value = true;
            cancelTokens.add(cancelRes);
            console.log(`${operationName}命令已发送，取消令牌:`, cancelRes);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            $q.notify({type: 'negative', message: formatError(e)});
        }
    }

    onUnmounted(async () => {
        if (cancelTokens.size === 0) return;

        const cancelPromises = Array.from(cancelTokens).map(token => api.cmdCancelTask(token));
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
        handleAudioRecorded: async (mimetype: string, data: Uint8Array) => {
            showAudioDrawer.value = false;
            if (beforeClick()) return;

            const virtualName = `Audio_${new Date().toISOString().replace(/[:.]/g, '-')}.webm`;

            await uploadMemoryAttachment(virtualName, data, mimetype, true, (att, url) => {
                if (!editorContentRef.value) {
                    console.error('编辑器内容引用未定义，无法插入音频节点');
                    return;
                }
                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                editorContentRef.value.insertAudio(att.filename);
            });
        },
        insertPhoto: () => genericBatchUpload(true, PHOTO_TYPES, 'img', "image"),
        takePhoto: async () => {
            if (beforeClick()) return;
            const key = uuidv4();
            uploadTaskMap.value[key] = {filename: 'take photo', progress: 0, status: 'pending'};
            const event = createUploadChannel(key, (meta, url) => {
                if (!editorContentRef.value) {
                    console.error('编辑器内容引用未定义，无法插入图片节点');
                    return;
                }
                currentDiaryAttachmentUrlMap.value[meta.filename] = url;
                editorContentRef.value.insertImage(meta.filename);
            });
            try {
                const res = await api.cmdAddImageAttachmentFromCamera(event, diaryId.value, true);
                cancelTokens.add(res);
            } catch (e) {
                uploadTaskMap.value[key].status = 'error';
                console.error("调用 Rust 后端失败:", formatError(e));
            }
        },
        audioRecording: () => {
            if (beforeClick()) return;
            showAudioDrawer.value = true;
        },
        insertAudio: () => genericBatchUpload(false, AUDIO_TYPES, 'audio'),
        insertVideo: () => genericBatchUpload(false, VIDEO_TYPES, 'video', "video"),
        insertFile: async () => genericBatchUpload(true),
        cachingAttachment: async (filenames: string[]) => {
            if (!filenames.length) return;
            showUploadDialog.value = true;
            for (const filename of filenames) {
                const key = uuidv4();
                uploadTaskMap.value[key] = {filename, progress: 0, status: 'pending'};
                const event = createUploadChannel(key);
                try {
                    const cancelToken = await api.cmdCachingAttachment(event, diaryId.value, filename);
                    cancelTokens.add(cancelToken);
                } catch (e) {
                    uploadTaskMap.value[key].status = 'error';
                    $q.notify({type: 'negative', message: `缓存 ${filename} 失败: ${formatError(e)}`});
                    console.error(`缓存 ${filename} 失败:`, e);
                }
            }
        },
        // 保存解密附件
        saveDecryptAttachment: async (filename: string) => await performAttachmentOperation(
            filename,
            '保存解密附件',
            api.cmdSaveDecryptAttachment
        ),
        // 切换附件加密状态
        toggleAttachmentEncryption: async (filename: string) => await performAttachmentOperation(
            filename,
            '切换附件加密',
            api.cmdToggleAttachmentEncryption
        ),
        // 旋转图片附件
        async rotateAttachment(filename: string, rotation: number) {
            // 验证旋转角度
            if ([90, 180, -90].indexOf(rotation) === -1) {
                console.log(`无效的旋转角度: ${rotation}`);
                $q.notify({type: 'negative', message: '无效的旋转角度'});
                return;
            }

            await performAttachmentOperation(
                filename,
                '旋转图片',
                api.cmdRotateImageAttachment,
                rotation
            );
        },
        async pasteAttachments(files: File[]) {
            if (!editorContentRef.value) {
                console.error('编辑器内容引用未定义，无法插入媒体节点');
                return;
            }
            console.log('粘贴的文件列表:', files.length);
            showUploadDialog.value = true;
            uploadTaskMap.value = {};
            const uploads = Array.from(files).map(file => {
                return new Promise<void>((resolve) => {
                    const reader = new FileReader();
                    reader.onload = async () => {
                        const arrayBuffer = reader.result as ArrayBuffer;
                        const uint8Array = new Uint8Array(arrayBuffer);
                        await uploadMemoryAttachment(file.name, uint8Array, file.type, false, (att, url) => {
                            if (!editorContentRef.value) {
                                console.error('编辑器内容引用未定义，无法插入媒体节点');
                                return;
                            }
                            if (file.type.startsWith('image/')) {
                                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                                editorContentRef.value.insertImage(att.filename);
                            } else if (file.type.startsWith('audio/')) {
                                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                                editorContentRef.value.insertAudio(att.filename);
                            } else if (file.type.startsWith('video/')) {
                                currentDiaryAttachmentUrlMap.value[att.filename] = url;
                                editorContentRef.value.insertVideo(att.filename);
                            } else {
                                editorContentRef.value.insertFile(att.filename);
                            }
                            resolve();
                        });
                    };
                    reader.onerror = () => {
                        $q.notify({type: 'negative', message: `${file.name} 读取失败`});
                        resolve();
                    };
                    reader.readAsArrayBuffer(file);
                });
            });
            await Promise.allSettled(uploads);
        },
    };
}

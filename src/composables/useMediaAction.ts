import {Channel} from "@tauri-apps/api/core";
import {platform} from "@tauri-apps/plugin-os";
import {AttachmentMeta, AttachmentProcessEvent} from "../bindings.ts";
import {computed, onUnmounted, Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog";
import {useQuasar} from "quasar";
import {v4 as uuidv4} from "uuid";
import {useDataStore} from "../stores/data.ts";
import {useConfigStore} from "../stores/config.ts";
import {storeToRefs} from "pinia";
import TiptapEditor from "../components/TiptapEditor.vue";
import api from "../utils/api.ts";
import {formatError} from "../utils/formatError.ts";
import {batchUploadAll, promisifyUpload} from "../utils/batchUpload";
import {
    attachmentNodeKindFromMimeType,
    applyAttachmentInsertions,
    planAttachmentInsertions,
    type AttachmentNodeKind,
    type UploadedAttachment,
} from "../utils/attachmentInsertion";

const CHUNK_SIZE = 5 * 1024 * 1024; // 5MB (S3 最小分片大小)

export interface UploadTask {
    filename: string;
    progress: number;
    status: 'pending' | 'uploading' | 'completed' | 'error';
    phase: 'preparing' | 'transferring' | 'finalizing';
}

type OnAttachmentProcessSuccess = (meta: AttachmentMeta, url: string) => void;

const PHOTO_TYPES = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'];
const AUDIO_TYPES = ['mp3', 'wav', 'ogg', 'flac', 'aac', 'm4a'];
const VIDEO_TYPES = ['mp4', 'avi', 'mov', 'mkv', 'webm'];

export function useMediaAction(
    diaryId: Ref<string>,
    editorDomRef: Ref<HTMLElement | undefined>,
    showPanel: Ref<boolean>,
    editorContentRef: Ref<InstanceType<typeof TiptapEditor> | undefined>
) {
    const $q = useQuasar();
    const dataStore = useDataStore();
    const configStore = useConfigStore();
    const {currentDiaryAttachmentUrlMap, currentDiary} = storeToRefs(dataStore);
    const attachmentEncryptionByKind: Record<AttachmentNodeKind, Ref<boolean>> = {
        image: configStore.useTauriConfig('encrypt_image_attachments'),
        audio: configStore.useTauriConfig('encrypt_audio_attachments'),
        video: configStore.useTauriConfig('encrypt_video_attachments'),
        file: configStore.useTauriConfig('encrypt_file_attachments'),
    };

    const cancelTokens = new Set<string>();
    const chunkedUploadTokens = new Set<string>();

    // 进度管理状态
    const uploadTaskMap = ref<Record<string, UploadTask>>({});
    const showUploadDialog = ref(false);
    const showAudioDrawer = ref(false);

    const uploadTasks = computed(() => Object.values(uploadTaskMap.value));
    const isUploading = computed(() => {
        if (uploadTasks.value.length === 0) return true;
        return uploadTasks.value.every(task => task.status === 'completed' || task.status === 'error');
    });

    function createUploadChannel(key: string, onSuccess?: OnAttachmentProcessSuccess, onError?: (errorMsg: string) => void) {
        const event = new Channel<AttachmentProcessEvent>();
        event.onmessage = (msg) => {
            const task = uploadTaskMap.value[key];
            if (!task) return;

            switch (msg.event) {
                case "started":
                    task.status = 'uploading';
                    task.phase = 'transferring';
                    break;
                case "progress":
                    task.progress = msg.data / 100;
                    break;
                case "finalizing":
                    task.phase = 'finalizing';
                    task.progress = Math.max(task.progress, 0.99);
                    break;
                case "completed":
                    task.status = 'completed';
                    task.progress = 1;
                    const [meta, url] = msg.data;
                    dataStore.updateAttachment(diaryId.value, meta);
                    onSuccess?.(meta, url);
                    break;
                case "completedWithoutData":
                    task.status = 'completed';
                    task.progress = 1;
                    break;
                case "error":
                    task.status = 'error';
                    $q.notify({type: 'negative', message: `${task.filename} 上传失败: ${msg.data}`});
                    onError?.(msg.data);
                    break;
            }
        };
        return event;
    }

    async function uploadAttachment(
        accessStr: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta, url: string) => void,
        errorCallback?: (errorMsg: string) => void
    ) {
        const rawName = accessStr.split(/[\\/]/).pop() || "未知文件";
        const key = uuidv4();
        uploadTaskMap.value[key] = {
            filename: rawName,
            progress: 0,
            status: 'pending',
            phase: 'preparing',
        };

        const event = createUploadChannel(key, completedCallback, errorCallback);

        try {
            const res = await api.cmdAddAttachment(event, diaryId.value, accessStr, encrypted, rawName);
            cancelTokens.add(res);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            console.error("调用 Rust 后端失败:", e);
            errorCallback?.(String(e));
        }
    }

    async function uploadAttachmentChunked(
        filename: string,
        totalSize: number,
        mimetype: string,
        encrypted: boolean,
        readChunk: (start: number, end: number) => Promise<Uint8Array>,
        completedCallback?: (meta: AttachmentMeta, url: string) => void,
        errorCallback?: (errorMsg: string) => void
    ) {
        const key = uuidv4();
        uploadTaskMap.value[key] = {
            filename,
            progress: 0,
            status: 'pending',
            phase: 'preparing',
        };
        showUploadDialog.value = true;

        let uploadToken: string | null = null;
        try {
            uploadTaskMap.value[key].status = 'uploading';
            uploadTaskMap.value[key].phase = 'transferring';

            // 初始化分片上传
            const startResult = await api.cmdStartChunkedUpload(
                diaryId.value, filename, mimetype, encrypted, totalSize
            );
            uploadToken = startResult.uploadToken;
            chunkedUploadTokens.add(uploadToken);

            // 逐片上传
            const totalChunks = Math.ceil(totalSize / CHUNK_SIZE);
            for (let i = 0; i < totalChunks; i++) {
                const start = i * CHUNK_SIZE;
                const end = Math.min(start + CHUNK_SIZE, totalSize);
                const bytes = await readChunk(start, end);
                if (bytes.length !== end - start) {
                    throw new Error(`读取分片大小不匹配：expected=${end - start}, actual=${bytes.length}`);
                }
                const chunk = Array.from(bytes);

                const chunkResult = await api.cmdUploadChunk(uploadToken, i, chunk);
                uploadTaskMap.value[key].progress = Math.min(
                    chunkResult.uploadedBytes / chunkResult.totalBytes,
                    0.99,
                );
            }

            // 完成上传
            uploadTaskMap.value[key].phase = 'finalizing';
            const finishResult = await api.cmdFinishChunkedUpload(uploadToken);
            uploadTaskMap.value[key].status = 'completed';
            uploadTaskMap.value[key].progress = 1;
            chunkedUploadTokens.delete(uploadToken);
            uploadToken = null;
            dataStore.updateAttachment(diaryId.value, finishResult.attachment);
            completedCallback?.(finishResult.attachment, finishResult.url);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            console.error("分片上传失败:", e);
            errorCallback?.(String(e));
            // 尝试取消服务端状态
            if (uploadToken) {
                chunkedUploadTokens.delete(uploadToken);
                api.cmdAbortChunkedUpload(uploadToken).catch(() => {});
            }
        }
    }

    function uploadMemoryAttachmentChunked(
        filename: string,
        bytes: Uint8Array,
        mimetype: string,
        encrypted: boolean,
        completedCallback?: (meta: AttachmentMeta, url: string) => void,
        errorCallback?: (errorMsg: string) => void
    ) {
        return uploadAttachmentChunked(
            filename,
            bytes.length,
            mimetype,
            encrypted,
            async (start, end) => bytes.slice(start, end),
            completedCallback,
            errorCallback,
        );
    }

    function beforeClick(opts?: { skipFocus?: boolean }) {
        if (!diaryId.value) {
            $q.notify({type: 'warning', message: '请先创建日记才能使用此功能'});
            return true;
        }
        if (showPanel.value) showPanel.value = false;
        uploadTaskMap.value = {};
        if (!opts?.skipFocus) {
            if (platform() !== 'android') {
                editorDomRef.value?.focus();
            }
        }
    }

    async function insertUploadedAttachments(
        results: (UploadedAttachment | null)[],
        atEnd = platform() !== 'android'
    ) {
        const editor = editorContentRef.value;
        if (!editor) return false;

        for (const item of results) {
            if (item && item.nodeKind !== 'file') {
                currentDiaryAttachmentUrlMap.value[item.attachmentId] = item.url;
            }
        }
        return applyAttachmentInsertions(
            planAttachmentInsertions(results, uuidv4),
            editor,
            atEnd,
        );
    }

    async function genericBatchUpload(
        encrypted: boolean,
        extensions?: string[],
        nodeKind: AttachmentNodeKind = 'file',
        pickerMode?: PickerMode
    ) {
        if (beforeClick()) return;
        const accessStrArr = await open({
            multiple: true,
            pickerMode: pickerMode,
            filters: extensions ? [{name: pickerMode || 'filter file', extensions}] : undefined
        });
        console.log('选中文件:', accessStrArr);
        if (!accessStrArr) return;

        showUploadDialog.value = true;

        const results = await batchUploadAll(accessStrArr, accessStr =>
            promisifyUpload<UploadedAttachment>((onSuccess, onError) => {
                uploadAttachment(accessStr, encrypted,
                    (meta, url) => onSuccess({
                        nodeKind,
                        attachmentId: meta.id,
                        filename: meta.filename,
                        url,
                    }),
                    onError
                );
            })
        );
        await insertUploadedAttachments(results);
    }

    async function performAttachmentOperation<Args extends any[]>(
        attachmentId: string,
        operationName: string,
        apiCall: (event: Channel<AttachmentProcessEvent>, diaryId: string, attachmentId: string, ...args: Args) => Promise<string>,
        ...apiArgs: Args
    ) {
        if (!diaryId.value.trim() || !attachmentId.trim()) {
            $q.notify({type: 'negative', message: `无法获取日记ID或附件ID，无法执行${operationName}`});
            return;
        }

        uploadTaskMap.value = {};
        editorDomRef.value?.focus();

        const key = uuidv4();
        const displayFilename = currentDiary.value?.attachments
            .find(attachment => attachment.id === attachmentId)?.filename || attachmentId;
        uploadTaskMap.value[key] = {
            filename: displayFilename,
            progress: 0,
            status: 'pending',
            phase: 'preparing',
        };

        const event = createUploadChannel(key, (_meta, url) => {
            if (!editorContentRef.value) {
                console.error('编辑器内容引用未定义，无法更新媒体链接');
                $q.notify({type: 'negative', message: '编辑器内容引用未定义，无法更新媒体链接'});
                return;
            }
            const res = editorContentRef.value.updateSrc(attachmentId, url);
            if (!res) {
                console.warn('未找到对应的附件元素，无法更新链接:', attachmentId);
            }
        });

        try {
            const cancelRes = await apiCall(event, diaryId.value, attachmentId, ...apiArgs);
            showUploadDialog.value = true;
            cancelTokens.add(cancelRes);
            console.log(`${operationName}命令已发送，取消令牌:`, cancelRes);
        } catch (e) {
            uploadTaskMap.value[key].status = 'error';
            $q.notify({type: 'negative', message: formatError(e)});
        }
    }

    onUnmounted(async () => {
        const promises: Promise<any>[] = [];

        if (cancelTokens.size > 0) {
            promises.push(...Array.from(cancelTokens).map(token => api.cmdCancelTask(token)));
        }
        if (chunkedUploadTokens.size > 0) {
            promises.push(...Array.from(chunkedUploadTokens).map(token => api.cmdAbortChunkedUpload(token)));
        }

        if (promises.length > 0) {
            const results = await Promise.allSettled(promises);
            for (const result of results) {
                if (result.status === "rejected" || (result.status === "fulfilled" && (result.value as any)?.status === "error")) {
                    console.error("取消上传任务失败:", result);
                }
            }
        }

        cancelTokens.clear();
        chunkedUploadTokens.clear();
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

            await uploadMemoryAttachmentChunked(
                virtualName,
                data,
                mimetype,
                attachmentEncryptionByKind.audio.value,
                (att, url) => {
                    if (!editorContentRef.value) {
                        console.error('编辑器内容引用未定义，无法插入音频节点');
                        return;
                    }
                    currentDiaryAttachmentUrlMap.value[att.id] = url;
                    editorContentRef.value.insertAudio(att.id);
                },
            );
        },
        insertPhoto: () => genericBatchUpload(
            attachmentEncryptionByKind.image.value,
            PHOTO_TYPES,
            'image',
            "image"
        ),
        takePhoto: async () => {
            if (beforeClick()) return;
            const key = uuidv4();
            uploadTaskMap.value[key] = {
                filename: 'take photo',
                progress: 0,
                status: 'pending',
                phase: 'preparing',
            };
            const event = createUploadChannel(key, (meta, url) => {
                if (!editorContentRef.value) {
                    console.error('编辑器内容引用未定义，无法插入图片节点');
                    return;
                }
                currentDiaryAttachmentUrlMap.value[meta.id] = url;
                editorContentRef.value.insertImage(meta.id);
            });
            try {
                const res = await api.cmdAddImageAttachmentFromCamera(
                    event,
                    diaryId.value,
                    attachmentEncryptionByKind.image.value,
                );
                cancelTokens.add(res);
            } catch (e) {
                uploadTaskMap.value[key].status = 'error';
                console.error("调用 Rust 后端失败:", formatError(e));
            }
        },
        audioRecording: () => {
            if (beforeClick({ skipFocus: true })) return;
            showAudioDrawer.value = true;
        },
        insertAudio: () => genericBatchUpload(
            attachmentEncryptionByKind.audio.value,
            AUDIO_TYPES,
            'audio'
        ),
        insertVideo: () => genericBatchUpload(
            attachmentEncryptionByKind.video.value,
            VIDEO_TYPES,
            'video',
            "video"
        ),
        insertFile: async () => genericBatchUpload(attachmentEncryptionByKind.file.value),
        cachingAttachment: async (attachmentIds: string[]) => {
            if (!attachmentIds.length) return;
            showUploadDialog.value = true;
            for (const attachmentId of attachmentIds) {
                const key = uuidv4();
                const filename = currentDiary.value?.attachments
                    .find(attachment => attachment.id === attachmentId)?.filename || attachmentId;
                uploadTaskMap.value[key] = {
                    filename,
                    progress: 0,
                    status: 'pending',
                    phase: 'preparing',
                };
                const event = createUploadChannel(key);
                try {
                    const cancelToken = await api.cmdCachingAttachment(event, diaryId.value, attachmentId);
                    cancelTokens.add(cancelToken);
                } catch (e) {
                    uploadTaskMap.value[key].status = 'error';
                    $q.notify({type: 'negative', message: `缓存 ${filename} 失败: ${formatError(e)}`});
                    console.error(`缓存 ${filename} 失败:`, e);
                }
            }
        },
        // 保存解密附件
        saveDecryptAttachment: async (attachmentId: string) => await performAttachmentOperation(
            attachmentId,
            '保存解密附件',
            api.cmdSaveDecryptAttachment
        ),
        // 切换附件加密状态
        toggleAttachmentEncryption: async (attachmentId: string) => await performAttachmentOperation(
            attachmentId,
            '切换附件加密',
            api.cmdToggleAttachmentEncryption
        ),
        // 旋转图片附件
        async rotateAttachment(attachmentId: string, rotation: number) {
            // 验证旋转角度
            if ([90, 180, -90].indexOf(rotation) === -1) {
                console.log(`无效的旋转角度: ${rotation}`);
                $q.notify({type: 'negative', message: '无效的旋转角度'});
                return;
            }

            await performAttachmentOperation(
                attachmentId,
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

            const results = await batchUploadAll(Array.from(files), file => {
                const detectedNodeKind = attachmentNodeKindFromMimeType(file.type);
                return promisifyUpload<UploadedAttachment>(
                    (onSuccess, onError) => {
                        void uploadAttachmentChunked(
                            file.name,
                            file.size,
                            file.type,
                            attachmentEncryptionByKind[detectedNodeKind].value,
                            async (start, end) => new Uint8Array(
                                await file.slice(start, end).arrayBuffer()
                            ),
                            (meta, url) => onSuccess({
                                nodeKind: attachmentNodeKindFromMimeType(meta.mimetype),
                                attachmentId: meta.id,
                                filename: meta.filename,
                                url,
                            }),
                            () => onError(),
                        );
                    }
                );
            });

            await insertUploadedAttachments(results);
        },
        insertExistingAttachmentsAtEnd(attachments: AttachmentMeta[]) {
            return insertUploadedAttachments(attachments.map(attachment => ({
                nodeKind: attachmentNodeKindFromMimeType(attachment.mimetype),
                attachmentId: attachment.id,
                filename: attachment.filename,
                url: currentDiaryAttachmentUrlMap.value[attachment.id] || '',
            })), true);
        },
    };
}

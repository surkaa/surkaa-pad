import {Channel} from "@tauri-apps/api/core";
import {platform} from "@tauri-apps/plugin-os";
import {AttachmentMeta, AttachmentProcessEvent} from "../bindings.ts";
import {Ref, ref} from "vue";
import {open, PickerMode} from "@tauri-apps/plugin-dialog";
import {useQuasar} from "quasar";
import {v4 as uuidv4} from "uuid";
import {useDataStore} from "../stores/data.ts";
import {useConfigStore} from "../stores/config.ts";
import {storeToRefs} from "pinia";
import TiptapEditor from "../components/TiptapEditor.vue";
import api from "../utils/api.ts";
import {formatError} from "../utils/formatError.ts";
import {formatBytes} from '../utils/format';
import {partitionAttachmentsByCacheLimit} from '../utils/attachmentCache';
import {batchUploadAll, promisifyUpload} from "../utils/batchUpload";
import {
    attachmentNodeKindFromMimeType,
    applyAttachmentInsertions,
    planAttachmentInsertions,
    type AttachmentNodeKind,
    type UploadedAttachment,
} from "../utils/attachmentInsertion";
import {useAttachmentUploader} from './useAttachmentUploader';

export type {UploadTask} from './useAttachmentUploader';

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
    const {currentDiaryAttachments, currentDiaryAttachmentUrlMap} = storeToRefs(dataStore);
    const attachmentEncryptionByKind: Record<AttachmentNodeKind, Ref<boolean>> = {
        image: configStore.useTauriConfig('encrypt_image_attachments'),
        audio: configStore.useTauriConfig('encrypt_audio_attachments'),
        video: configStore.useTauriConfig('encrypt_video_attachments'),
        file: configStore.useTauriConfig('encrypt_file_attachments'),
    };
    const uploadConcurrency = configStore.useTauriConfig('attachment_upload_concurrency');

    const showAudioDrawer = ref(false);
    const {
        uploadTasks,
        showUploadDialog,
        hasActiveUploads,
        allUploadsSettled,
        createTask,
        createUploadChannel,
        failTask,
        registerCancelableTask,
        resetUploadTasks,
        cancelUploadTask,
        cancelAllUploads,
        uploadAttachment,
        uploadAttachmentChunked,
        uploadMemoryAttachmentChunked,
    } = useAttachmentUploader(diaryId);

    function beforeClick(opts?: { skipFocus?: boolean }) {
        if (!diaryId.value) {
            $q.notify({type: 'warning', message: '请先创建日记才能使用此功能'});
            return true;
        }
        if (showPanel.value) showPanel.value = false;
        if (!resetUploadTasks()) {
            showUploadDialog.value = true;
            $q.notify({type: 'warning', message: '请先等待当前文件处理完成或取消任务'});
            return true;
        }
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
        const queuedUploads = accessStrArr.map(accessStr => ({
            accessStr,
            taskId: createTask(accessStr.split(/[\\/]/).pop() || '未知文件', true),
        }));

        const results = await batchUploadAll(queuedUploads, ({accessStr, taskId}) =>
            promisifyUpload<UploadedAttachment>((onSuccess, onError) => {
                uploadAttachment(accessStr, encrypted,
                    (meta, url) => onSuccess({
                        nodeKind,
                        attachmentId: meta.id,
                        filename: meta.filename,
                        url,
                    }),
                    onError,
                    taskId,
                );
            }),
            uploadConcurrency.value,
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

        if (!resetUploadTasks()) {
            showUploadDialog.value = true;
            $q.notify({type: 'warning', message: '请先等待当前文件处理完成或取消任务'});
            return;
        }
        editorDomRef.value?.focus();

        const displayFilename = currentDiaryAttachments.value
            .find(attachment => attachment.id === attachmentId)?.filename || attachmentId;
        const key = createTask(displayFilename);

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
            registerCancelableTask(key, cancelRes);
            console.log(`${operationName}命令已发送，取消令牌:`, cancelRes);
        } catch (e) {
            failTask(key, e);
        }
    }

    return {
        uploadTasks,
        showUploadDialog,
        hasActiveUploads,
        allUploadsSettled,
        cancelUploadTask,
        cancelAllUploads,
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
            const key = createTask('take photo');
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
                showUploadDialog.value = true;
                registerCancelableTask(key, res);
            } catch (e) {
                failTask(key, e);
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
            let cacheableAttachmentIds = attachmentIds;
            try {
                const cacheInfo = await api.cmdGetAttachmentCacheInfo();
                const partition = partitionAttachmentsByCacheLimit(
                    attachmentIds,
                    currentDiaryAttachments.value,
                    cacheInfo.maxFileSizeBytes,
                );
                cacheableAttachmentIds = partition.cacheableIds;
                if (partition.oversizedIds.length) {
                    $q.notify({
                        type: 'warning',
                        message: `${partition.oversizedIds.length} 个附件超过单个附件缓存上限（${formatBytes(cacheInfo.maxFileSizeBytes)}），已跳过；请在设置中调高后重试`,
                    });
                }
            } catch (error) {
                // 本地模式没有缓存配置，仍沿用原有操作；远端会由后端再次校验。
                console.debug('未读取单个附件缓存上限，将交由后端校验:', error);
            }
            if (!cacheableAttachmentIds.length) return;
            if (!resetUploadTasks()) {
                showUploadDialog.value = true;
                $q.notify({type: 'warning', message: '请先等待当前文件处理完成或取消任务'});
                return;
            }
            showUploadDialog.value = true;
            for (const attachmentId of cacheableAttachmentIds) {
                const filename = currentDiaryAttachments.value
                    .find(attachment => attachment.id === attachmentId)?.filename || attachmentId;
                const key = createTask(filename, false, 'download');
                const event = createUploadChannel(key);
                try {
                    const cancelToken = await api.cmdCachingAttachment(event, diaryId.value, attachmentId);
                    registerCancelableTask(key, cancelToken);
                } catch (e) {
                    failTask(key, e, undefined, false);
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
            if (!resetUploadTasks()) {
                showUploadDialog.value = true;
                $q.notify({type: 'warning', message: '请先等待当前文件处理完成或取消任务'});
                return;
            }
            showUploadDialog.value = true;
            const queuedUploads = Array.from(files, file => ({
                file,
                taskId: createTask(file.name, true),
            }));

            const results = await batchUploadAll(queuedUploads, ({file, taskId}) => {
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
                            taskId,
                        );
                    }
                );
            }, uploadConcurrency.value);

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

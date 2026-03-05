import {AttachmentMeta} from "./bindings.ts";

export type DiaryChangedEvent =
    | { type: 'created'; summary: DiarySummary }
    | { type: 'updated'; summary: DiarySummary }
    | { type: 'updated-attachment-encryption'; filename: string; encrypted: boolean }
    | { type: 'deleted'; id: string };

export type AttachmentEncryptionChangeEvent = {
    diaryId: string;
    meta: AttachmentMeta,
    url: string;
}

export type OssConfigType = {
    akid: string;
    aks: string;
    bucket: string;
    endpoint: string;
}

export type ThemeType = 'light' | 'dark' | 'system';

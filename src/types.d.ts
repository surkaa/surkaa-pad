export type DiaryManifest = {
    id: string;
    // 加密算法名称
    algorithm: string;
    // 日记正文
    content: string;
    created: number;
    updated: number;
    // 附件列表
    attachments: AttachmentMeta[];
}

// 单个附件的元数据
export type AttachmentMeta = {
    filename: string;
    mimetype: string;
    size: number;
    // 用于加密该文件的独立 IV
    nonce: number[];
}

export type OssConfigType = {
    akid: string;
    aks: string;
    bucket: string;
    endpoint: string;
}
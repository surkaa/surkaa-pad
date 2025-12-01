export type DiaryManifest = {
    id: string;
    // 加密算法名称
    algorithm: string;
    // 日记正文
    content: string;
    createdAt: number;
    updatedAt: number;
    // 附件列表
    attachments: AttachmentMeta[];
}

// 单个附件的元数据
export type AttachmentMeta = {
    fileName: string;
    mimeType: string;
    size: number;
    // 用于加密该文件的独立 IV
    nonce: number[];
}

export type OssConfigType = {
    accessKeyId: string;
    accessKeySecret: string;
    bucketName: string;
    endpoint: string;
}
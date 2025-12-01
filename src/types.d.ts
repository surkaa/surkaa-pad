export type DiaryEntry = {
    id: number;
    nonce: number[];
}

export type PageSearchResult = {
    id: number;
    content: string;
}

export type SearchIndexResult = {
    id: number;
    count: number;
}

export type KeywordToken = {
    word: string;
    count: number;
}

export type BatchIndexEntry = {
    id: number;
    search_hash: number[];
    count: number;
}

/**
 * 日记文件头结构
 */
export type DiaryFileHeader = {
    totalLength: number;
    algorithm: string;
    nonce: number[]; // IV (12 字节)
    encHash: number[]; // 加密内容哈希 (32 字节)
}

/**
 * 后端上传的加密数据结构
 */
export type EncryptData = {
    total_length: number;
    algorithm: string;
    nonce: number[];
    ciphertext: number[];
    enc_hash: number[];
}

export type OssConfigType = {
    accessKeyId: string;
    accessKeySecret: string;
    bucketName: string;
    endpoint: string;
}
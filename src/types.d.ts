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
export interface DiaryFileHeader {
    totalLength: number;
    algorithm: string;
    nonce: number[]; // IV (12 字节)
    encHash: number[]; // 加密内容哈希 (32 字节)
}
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
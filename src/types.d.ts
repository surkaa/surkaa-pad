export type DiaryEntry = {
    id: number;
    created_at: string;
    nonce: number[];
}

export type SearchResult = {
    id: number;
    created_at: string;
    nonce: number[];
    content: string;
}
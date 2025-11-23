export type DiaryEntry = {
    id: number;
    created_at: number;
    nonce: number[];
}

export type SearchResult = {
    id: number;
    created_at: number;
    content: string;
}
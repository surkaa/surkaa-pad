export type DiaryChangedEvent =
    | { type: 'created'; summary: DiarySummary }
    | { type: 'updated'; summary: DiarySummary }
    | { type: 'deleted'; id: string };

export type OssConfigType = {
    akid: string;
    aks: string;
    bucket: string;
    endpoint: string;
}

export type ThemeType = 'light' | 'dark' | 'system';

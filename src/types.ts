export type OssConfigType = {
    akid: string;
    aks: string;
    bucket: string;
    endpoint: string;
}

export type ThemeType = 'light' | 'dark' | 'system';
export const DEFAULT_THEME: ThemeType = 'system';

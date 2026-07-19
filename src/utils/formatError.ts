export function formatError(e: any): string {
    if (e instanceof Error) {
        return e.message;
    }
    if (typeof e === 'string') {
        return e;
    }
    if (e && typeof e.message === 'string') {
        return e.message;
    }
    return JSON.stringify(e);
}

export const NEWER_DIARY_VERSION_MESSAGE = '发现由更高版本应用创建的日记，请升级应用后再查看';

export function isNewerDiaryVersionError(e: unknown): boolean {
    return typeof e === 'object'
        && e !== null
        && 'error_type' in e
        && e.error_type === 'diary_version_too_new';
}

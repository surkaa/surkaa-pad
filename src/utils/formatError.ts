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
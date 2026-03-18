export function formatError(e: any): string {
    if (e instanceof Error) {
        return e.message;
    }
    if (typeof e === 'string') {
        return e;
    }
    return JSON.stringify(e);
}
export function formatTimestamp(timestamp: number) {
    if (!timestamp) return 'N/A';
    return new Date(timestamp).toLocaleString('zh-CN', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit'
    });
}
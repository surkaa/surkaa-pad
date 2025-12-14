/**
 * 格式化时间戳，返回相对时间（如“5分钟前”）或简化的日期格式。
 * @param timestamp - 要格式化的时间戳（毫秒）。
 * @returns 格式化后的字符串，或 'N/A'。
 */
export function formatTimestamp(timestamp?: number): string {
    if (!timestamp) return 'N/A';

    const now = Date.now();
    const past = new Date(timestamp).getTime();
    const diff = now - past; // 时间差（毫秒）

    const minute = 60 * 1000;
    const hour = minute * 60;
    const day = hour * 24;
    const year = day * 365;

    // --- 相对时间格式 ---

    // 1. 几秒前
    if (diff < minute) {
        // 如果是1分钟内，返回“刚刚”
        return '刚刚';
    }
    // 2. 几分钟前 (1小时内)
    if (diff < hour) {
        const minutes = Math.floor(diff / minute);
        return `${minutes}分钟前`;
    }
    // 3. 几小时前 (24小时内)
    if (diff < day) {
        const hours = Math.floor(diff / hour);
        return `${hours}小时前`;
    }

    // --- 简化绝对时间格式 ---

    const date = new Date(timestamp);

    // 4. 昨天
    const yesterday = new Date(now - day);
    if (date.getFullYear() === yesterday.getFullYear() && date.getMonth() === yesterday.getMonth() && date.getDate() === yesterday.getDate()) {
        return '昨天' + date.toLocaleTimeString('zh-CN', {hour: '2-digit', minute: '2-digit'});
    }

    // 5. 今年内（显示 月/日 时:分）
    if (diff < year) {
        return date.toLocaleString('zh-CN', {
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit'
        }).replace(/\//g, '-');
    }

    // 6. 超过一年（显示 年/月/日）
    return date.toLocaleString('zh-CN', {year: 'numeric', month: '2-digit', day: '2-digit'}).replace(/\//g, '-');
}

/**
 * 格式化比特大小，返回带单位的字符串表示。
 * @param bytes - 要格式化的字节数。
 * @returns 格式化后的字符串，或 'N/A'。
 */
export function formatBytes(bytes?: number): string {
    if (bytes === undefined || bytes === null) return 'N/A';

    const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];
    let index = 0;
    let size = bytes;
    while (size >= 1024 && index < units.length - 1) {
        size /= 1024;
        index++;
    }
    return `${size.toFixed(0)} ${units[index]}`;
}
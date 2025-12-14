export function debounce<T extends (...args: any[]) => any>(
    func: T,
    wait: number
): T & { cancel: () => void } {
    let timeout: number | null = null;

    const debounced = function (...args: Parameters<T>) {
        if (timeout) {
            clearTimeout(timeout);
        }

        timeout = setTimeout(() => {
            func(...args);
            timeout = null;
        }, wait);
    } as T;

    // 添加取消方法
    (debounced as any).cancel = () => {
        if (timeout) {
            clearTimeout(timeout);
            timeout = null;
        }
    };

    return debounced as T & { cancel: () => void };
}
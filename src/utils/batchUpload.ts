/**
 * 将基于回调的上传函数包装为 Promise，成功 resolve 结果、失败 resolve null。
 * 确保 Promise 不会 reject，适合配合 Promise.all 做有序批量上传。
 */
export function promisifyUpload<T>(
    executor: (
        onSuccess: (result: T) => void,
        onError: () => void
    ) => void
): Promise<T | null> {
    return new Promise<T | null>(resolve => {
        executor(
            (result) => resolve(result),
            () => resolve(null)
        );
    });
}

/**
 * 并行执行所有上传，按原始数组顺序返回结果。
 * 每个上传函数应返回 Promise<T | null>，失败时返回 null。
 */
export async function batchUploadAll<T>(
    items: T[],
    uploadFn: (item: T) => Promise<unknown | null>
): Promise<(unknown | null)[]> {
    return Promise.all(items.map(uploadFn));
}

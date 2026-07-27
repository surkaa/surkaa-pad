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
 * 使用有界并发执行上传，按原始数组顺序返回结果。
 * 每个上传函数应返回 Promise<T | null>，失败时返回 null。
 */
export async function batchUploadAll<T, R>(
    items: T[],
    uploadFn: (item: T) => Promise<R | null>,
    concurrency = 2,
): Promise<(R | null)[]> {
    if (!Number.isInteger(concurrency) || concurrency < 1) {
        throw new Error('concurrency must be a positive integer');
    }

    const results = new Array<R | null>(items.length);
    let nextIndex = 0;
    async function worker() {
        while (nextIndex < items.length) {
            const index = nextIndex++;
            results[index] = await uploadFn(items[index]);
        }
    }

    const workerCount = Math.min(concurrency, items.length);
    await Promise.all(Array.from({length: workerCount}, () => worker()));
    return results;
}

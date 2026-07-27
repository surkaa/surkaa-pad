import { describe, it, expect } from 'vitest';
import { promisifyUpload, batchUploadAll } from '../batchUpload';

describe('promisifyUpload', () => {
    it('成功时 resolve 上传结果', async () => {
        const result = await promisifyUpload<{ name: string }>((onSuccess) => {
            onSuccess({ name: 'test.jpg' });
        });
        expect(result).toEqual({ name: 'test.jpg' });
    });

    it('失败时 resolve null', async () => {
        const result = await promisifyUpload<string>((_onSuccess, onError) => {
            onError();
        });
        expect(result).toBeNull();
    });

    it('不会 reject——始终 resolve', async () => {
        const promise = promisifyUpload<string>((_onSuccess, onError) => {
            onError();
        });
        // 如果会 reject，这行会直接抛异常
        const result = await promise;
        expect(result).toBeNull();
    });
});

describe('batchUploadAll', () => {
    it('按原始顺序返回结果', async () => {
        const items = ['a', 'b', 'c'];
        const results = await batchUploadAll(items, (item) =>
            Promise.resolve(`uploaded-${item}`)
        );
        expect(results).toEqual(['uploaded-a', 'uploaded-b', 'uploaded-c']);
    });

    it('失败项返回 null，成功项返回数据，顺序不变', async () => {
        const items = ['a', 'b', 'c'];
        const uploadFn = (item: string) =>
            item === 'b' ? Promise.resolve(null) : Promise.resolve(`ok-${item}`);

        const results = await batchUploadAll(items, uploadFn);
        expect(results).toEqual(['ok-a', null, 'ok-c']);
    });

    it('即使完成时间不同也保持原始顺序', async () => {
        const items = [1, 2, 3];
        const uploadFn = (item: number) =>
            new Promise<string>(resolve => {
                // 第 3 个最先完成，第 1 个最后完成
                const delay = item === 1 ? 30 : item === 2 ? 20 : 10;
                setTimeout(() => resolve(`item-${item}`), delay);
            });

        const results = await batchUploadAll(items, uploadFn);
        // Promise.all 保证结果按输入顺序返回
        expect(results).toEqual(['item-1', 'item-2', 'item-3']);
    });

    it('空数组返回空数组', async () => {
        const results = await batchUploadAll([], () => Promise.resolve('x'));
        expect(results).toEqual([]);
    });

    it('限制同时执行的上传数量', async () => {
        let active = 0;
        let maxActive = 0;
        const results = await batchUploadAll([1, 2, 3, 4, 5], async item => {
            active += 1;
            maxActive = Math.max(maxActive, active);
            await new Promise(resolve => setTimeout(resolve, 5));
            active -= 1;
            return item * 2;
        }, 2);

        expect(maxActive).toBe(2);
        expect(results).toEqual([2, 4, 6, 8, 10]);
    });

    it('拒绝无效的并发数量', async () => {
        await expect(batchUploadAll([1], async item => item, 0))
            .rejects.toThrow('concurrency must be a positive integer');
    });

    it('配合 promisifyUpload 使用——按序完成', async () => {
        const items = [
            { id: 1, name: 'first' },
            { id: 2, name: 'second' },
            { id: 3, name: 'third' },
        ];

        const uploadFn = (item: { id: number; name: string }) =>
            promisifyUpload<{ id: number; originalName: string }>((onSuccess, onError) => {
                if (item.id === 2) {
                    onError();
                } else {
                    // 模拟不同完成时间
                    setTimeout(() => onSuccess({ id: item.id, originalName: item.name }), (4 - item.id) * 10);
                }
            });

        const results = await batchUploadAll(items, uploadFn);
        expect(results[0]).toEqual({ id: 1, originalName: 'first' });
        expect(results[1]).toBeNull();
        expect(results[2]).toEqual({ id: 3, originalName: 'third' });
    });
});

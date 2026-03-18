import {commands, Result} from '../bindings.ts';

// 类型体操：推断原始函数的返回值，如果是 Promise<Result<T, E>> 则提取出 Promise<T>，同时如果是 Promise<void> 则保持不变
type UnwrapResult<T> = T extends Promise<Result<infer U, any>> ? Promise<U> : T;

// 构造包装后的 API 类型
export type WrappedCommands = {
    [K in keyof typeof commands]: (
        ...args: Parameters<(typeof commands)[K]>
    ) => UnwrapResult<ReturnType<(typeof commands)[K]>>;
};

export const api = {} as WrappedCommands;

for (const key of Object.keys(commands) as Array<keyof typeof commands>) {
    const originalMethod = commands[key] as Function;

    api[key] = async (...args: any[]) => {
        const res = await originalMethod(...args);

        // 判断返回值是否为 tauri-specta 生成的 Result 结构
        if (res && typeof res === 'object' && 'status' in res) {
            if (res.status === 'error') {
                // 直接抛出错误，外部可以通过 try/catch 或 .catch() 捕获
                throw res.error;
            }
            // 成功则直接返回核心数据
            return res.data;
        }

        // 如果没有 status 字段 (例如 Promise<void> 直接返回 void)，则原样返回
        return res;
    };
}

export default api;

//@ts-ignore
import OSS from "ali-oss";

let client: InstanceType<typeof OSS> | null = null;

/**
 * 初始化 OSS 客户端
 */
export async function initOSS(config: any) {
    client = new OSS({
        region: config.region,
        accessKeyId: config.accessKeyId,
        accessKeySecret: config.accessKeySecret,
        authorizationV4: true,
        bucket: config.bucket,
        endpoint: config.endpoint,
    });

    // 前端验证连接
    await checkConnection();
    console.log("前端 OSS 客户端初始化完成");
}

/**
 * 验证连接 (列出 Bucket 文件，maxKeys 1)
 */
async function checkConnection(): Promise<void> {
    if (!client) throw new Error("OSS 未初始化");
    try {
        let a = await client.list();
        console.log(a);
    } catch (e) {
        throw new Error(`OSS 连接验证失败: ${e}`);
    }
}

/**
 * 上传文件 (支持 Rust 返回的加密字节数组)
 */
export async function uploadFile(objectKey: string, data: number[] | Uint8Array): Promise<void> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        // 转成 Buffer 格式
        const bufferData = data instanceof Uint8Array ? OSS.Buffer.from(data) : OSS.Buffer.from(data);
        await client.put(objectKey, bufferData);
        console.log(`文件上传成功: ${objectKey}, 大小: ${bufferData.length} 字节`);
    } catch (error) {
        throw new Error(`文件上传失败 (${objectKey}): ${error}`);
    }
}

/**
 * 下载文件 (返回 number[] 以便传回 Rust 解密)
 */
export async function downloadFile(objectKey: string): Promise<number[]> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        // 下载文件，指定返回类型为 buffer
        const result = await client.get(objectKey, {
            responseType: 'arraybuffer'
        });

        if (!result.content) {
            throw new Error("下载的文件内容为空");
        }

        // 将 ArrayBuffer 转换为 number[]
        const arrayBuffer = result.content as ArrayBuffer;
        const uint8Array = new Uint8Array(arrayBuffer);
        const numberArray = Array.from(uint8Array);

        console.log(`文件下载成功: ${objectKey}, 大小: ${numberArray.length} 字节`);
        return numberArray;
    } catch (error) {
        throw new Error(`文件下载失败 (${objectKey}): ${error}`);
    }
}

/**
 * 删除文件
 */
export async function deleteFile(objectKey: string): Promise<void> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        await client.delete(objectKey);
        console.log(`文件删除成功: ${objectKey}`);
    } catch (error) {
        throw new Error(`文件删除失败 (${objectKey}): ${error}`);
    }
}
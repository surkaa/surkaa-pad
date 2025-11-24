//@ts-ignore https://help.aliyun.com/zh/oss/developer-reference/node-js-1
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
        let bucket = await client.getBucket();
        console.log(bucket);
    } catch (e) {
        throw new Error(`OSS 连接验证失败: ${e}`);
    }
}

/**
 * 列出文件
 */
export async function listFiles(prefix: string = ''): Promise<string[]> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        const result = await client.list();

        const fileKeys = result.objects ? result.objects.map((obj: { name: string }) => obj.name) : [];
        console.log(`列出文件成功, 前缀: ${prefix}, 文件数: ${fileKeys.length}`);
        return fileKeys;
    } catch (error) {
        throw new Error(`列出文件失败 (${prefix}): ${error}`);
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
 * 下载文件的文件头 前四个字节是文件头长度 确定长度后再下载指定长度的文件头
 */
export async function downloadFileHead(objectKey: string): Promise<number[]> {
    if (!client) throw new Error("OSS 未初始化");
    try {
        // 下载文件的前 4 个字节
        const result = await client.get(objectKey, {
            responseType: 'arraybuffer',
            headers: {
                'Range': 'bytes=0-3'
            }
        });

        if (!result.content) {
            throw new Error("下载的文件头内容为空");
        }
        // 计算文件头长度
        const arrayBuffer = result.content as ArrayBuffer;
        const dataView = new DataView(arrayBuffer);
        const headerLength = dataView.getUint32(0, false); // 假设是大端序

        // 下载完整的文件头
        const fullHeaderResult = await client.get(objectKey, {
            responseType: 'arraybuffer',
            headers: {
                'Range': `bytes=0-${headerLength - 1}`
            }
        });

        if (!fullHeaderResult.content) {
            throw new Error("下载的完整文件头内容为空");
        }

        const fullHeaderArrayBuffer = fullHeaderResult.content as ArrayBuffer;
        const fullHeaderUint8Array = new Uint8Array(fullHeaderArrayBuffer);
        const numberArray = Array.from(fullHeaderUint8Array);

        console.log(`完整文件头下载成功: ${objectKey}, 文件头长度: ${numberArray.length} 字节`);
        return numberArray;
    } catch (error) {
        throw new Error(`完整文件头下载失败 (${objectKey}): ${error}`);
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
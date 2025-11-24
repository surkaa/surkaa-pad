//@ts-ignore https://help.aliyun.com/zh/oss/developer-reference/node-js-1
import OSS from "ali-oss";
import {DiaryFileHeader} from "../types";

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
 * 下载文件的文件头，并解析成结构化对象
 * 流程：1. 下载前 2 字节 -> 2. 解析文件头长度 -> 3. 下载完整文件头 -> 4. 解析结构
 */
export async function downloadFileHead(objectKey: string): Promise<DiaryFileHeader> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        // --- 1. 下载文件的前 2 个字节 (索引 0 到 1) ---
        const result2Bytes = await client.get(objectKey, {
            responseType: 'arraybuffer',
            headers: {
                'Range': 'bytes=0-1'
            }
        });

        if (!result2Bytes.content || (result2Bytes.content as ArrayBuffer).byteLength !== 2) {
            throw new Error("下载前 2 字节内容不正确或为空");
        }

        // --- 2. 计算文件头长度 (使用 Big Endian, false) ---
        const arrayBuffer2Bytes = result2Bytes.content as ArrayBuffer;
        const dataView2Bytes = new DataView(arrayBuffer2Bytes);
        // ⭐ 修正点：使用 getUint16 来读取 2 字节长度
        const headerLength = dataView2Bytes.getUint16(0, false);

        // 校验最小长度 (2 字节长度 + 1 字节算法长度 + 12 字节 IV + 32 字节 Hash = 47 字节)
        const MIN_HEADER_LEN = 47;
        if (headerLength < MIN_HEADER_LEN) {
            throw new Error(`文件头长度 (${headerLength}) 小于最小预期长度 (${MIN_HEADER_LEN})。`);
        }
        // 校验最大长度 (65535)
        const MAX_HEADER_LEN = 65535;
        if (headerLength > MAX_HEADER_LEN) {
            throw new Error(`文件头长度 (${headerLength}) 超过最大预期长度 (${MAX_HEADER_LEN})。`);
        }

        // --- 3. 下载完整的文件头 ---
        const fullHeaderResult = await client.get(objectKey, {
            responseType: 'arraybuffer',
            headers: {
                'Range': `bytes=0-${headerLength - 1}`
            }
        });

        if (!fullHeaderResult.content) {
            throw new Error("下载的完整文件头内容为空");
        }

        // --- 4. 解析完整的文件头 ---
        const fullHeaderArrayBuffer = fullHeaderResult.content as ArrayBuffer;
        const fullHeaderUint8Array = new Uint8Array(fullHeaderArrayBuffer);
        const dataView = new DataView(fullHeaderArrayBuffer);

        let currentOffset = 2; // 跳过 2 字节的总长度

        // 4.1. 算法名称长度 (1 字节)
        const algoNameLength = dataView.getUint8(currentOffset);
        currentOffset += 1; // 移动到算法名开始处

        // 4.2. 算法名称 (变长)
        const algoNameBytes = fullHeaderUint8Array.slice(currentOffset, currentOffset + algoNameLength);
        const algorithm = new TextDecoder('utf-8').decode(algoNameBytes);
        currentOffset += algoNameLength;

        // 4.3. IV (Nonce) (12 字节)
        const NONCE_LEN = 12;
        const nonce = Array.from(fullHeaderUint8Array.slice(currentOffset, currentOffset + NONCE_LEN));
        currentOffset += NONCE_LEN;

        // 4.4. 加密内容哈希 (32 字节)
        const HASH_LEN = 32;
        const encHash = Array.from(fullHeaderUint8Array.slice(currentOffset, currentOffset + HASH_LEN));

        // --- 5. 返回结构化对象 ---
        console.log(`完整文件头解析成功: ${objectKey}, 算法: ${algorithm}, 长度: ${headerLength} 字节`);
        return {
            totalLength: headerLength,
            algorithm: algorithm,
            nonce: nonce,
            encHash: encHash,
        };
    } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        throw new Error(`文件头下载或解析失败 (${objectKey}): ${errorMessage}`);
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
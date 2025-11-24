//@ts-ignore https://help.aliyun.com/zh/oss/developer-reference/node-js-1
import OSS from "ali-oss";
import {DiaryFileHeader} from "../types";

let client: InstanceType<typeof OSS> | null = null;
// 引入 TextEncoder 用于将字符串转为字节
const encoder = new TextEncoder();

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
 * 上传文件
 */
async function uploadRawData(objectKey: string, data: number[] | Uint8Array): Promise<void> {
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
 * 封装的日记文件上传函数：自动组装文件头和密文数据
 * @param objectKey 文件路径/ID (例如: 1732388000000.dat)
 * @param header 文件头结构化数据
 * @param ciphertext 密文内容 (number[] 或 Uint8Array)
 */
export async function uploadDiaryFile(
    objectKey: string,
    header: DiaryFileHeader,
    ciphertext: number[] | Uint8Array
): Promise<void> {
    // 1. 验证总长度
    const ciphertextBytes = ciphertext instanceof Uint8Array ? ciphertext : new Uint8Array(ciphertext);
    const expectedLength = header.totalLength + ciphertextBytes.length;

    // 2. 组装文件头字节数据
    const headerBytes = assembleFileHeader(header);

    // 3. 校验最终文件头长度是否一致
    if (headerBytes.length !== header.totalLength) {
        throw new Error(`文件头组装失败：实际长度 ${headerBytes.length} 与预期长度 ${header.totalLength} 不符`);
    }

    // 4. 拼接最终上传数据 (文件头 + 密文)
    const finalData = new Uint8Array(expectedLength);
    finalData.set(headerBytes, 0);
    finalData.set(ciphertextBytes, headerBytes.length);

    // 5. 调用内部上传函数
    await uploadRawData(objectKey, finalData);
}


/**
 * 内部函数：根据 DiaryFileHeader 结构体，组装完整的字节数组
 */
function assembleFileHeader(header: DiaryFileHeader): Uint8Array {
    // 1. 算法名字节
    const algoNameBytes = encoder.encode(header.algorithm);
    const algoNameLength = algoNameBytes.length;

    // 2. 计算总长度 (确保 header.totalLength 是准确的)
    // 2 (长度) + 1 (算法名长度) + N (算法名) + 12 (IV) + 32 (Hash)
    // 假设此校验在 Rust 中完成，这里只使用 header.totalLength

    const buffer = new ArrayBuffer(header.totalLength);
    const dataView = new DataView(buffer);
    const uint8Array = new Uint8Array(buffer);
    let offset = 0;

    // 1. 文件头总长度 (2 字节, Big Endian)
    dataView.setUint16(offset, header.totalLength, false);
    offset += 2;

    // 2. 算法名称长度 (1 字节)
    dataView.setUint8(offset, algoNameLength);
    offset += 1;

    // 3. 算法名称 (变长)
    uint8Array.set(algoNameBytes, offset);
    offset += algoNameLength;

    // 4. IV (Nonce) (12 字节)
    uint8Array.set(new Uint8Array(header.nonce), offset);
    offset += 12;

    // 5. 加密内容哈希 (32 字节)
    uint8Array.set(new Uint8Array(header.encHash), offset);
    // offset += 32; // 结束

    return uint8Array;
}

/**
 * 下载文件 (返回 number[] 以便传回 Rust 解密)
 */
export async function downloadFile(objectKey: string, range?: {
    start: number;
    end: number;
}): Promise<number[]> {
    if (!client) throw new Error("OSS 未初始化");

    try {
        // 下载文件，指定返回类型为 buffer
        const result = await client.get(objectKey, {
            responseType: 'arraybuffer',
            headers: range ? {
                'Range': `bytes=${range.start}-${range.end}`
            } : undefined
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

        const content2Bytes = result2Bytes.content;
        if (!content2Bytes || (content2Bytes as ArrayBuffer).byteLength !== 2) {
            throw new Error("下载前 2 字节内容不正确或为空");
        }

        // --- 2. 计算文件头长度 (使用 Big Endian, false) ---
        const arrayBuffer2Bytes = content2Bytes instanceof Uint8Array ? content2Bytes.buffer : content2Bytes;
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

        const fullHeaderContent = fullHeaderResult.content;
        if (!fullHeaderContent) {
            throw new Error("下载的完整文件头内容为空");
        }

        // --- 4. 解析完整的文件头 ---
        const fullHeaderArrayBuffer = fullHeaderContent instanceof Uint8Array ? fullHeaderContent.buffer : fullHeaderContent;
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
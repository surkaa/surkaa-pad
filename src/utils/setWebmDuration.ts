// 仅修正 MediaRecorder 生成的流式 WebM；保留原始音轨与 Cluster 字节。
const SEGMENT_ID = 0x18538067;
const INFO_ID = 0x1549a966;
const DURATION_ID = 0x4489;
const TIMECODE_SCALE_ID = 0x2ad7b1;

interface ElementRange {
    id: number;
    start: number;
    dataStart: number;
    end: number;
    unknownSize: boolean;
}

function readVint(bytes: Uint8Array, offset: number, id = false) {
    const first = bytes[offset];
    if (!first) throw new Error('无效或截断的 WebM VINT');
    let width = 1;
    let marker = 0x80;
    while (!(first & marker)) { width += 1; marker /= 2; }
    if (width > (id ? 4 : 8) || offset + width > bytes.length) throw new Error('截断的 WebM VINT');
    let value = id ? first : first - marker;
    let unknown = !id && value === marker - 1;
    // 不能使用位运算累积：Chrome 的 8 字节“未知长度”会溢出为错误的 Segment 长度。
    for (let index = 1; index < width; index += 1) {
        value = value * 256 + bytes[offset + index];
        unknown = unknown && bytes[offset + index] === 0xff;
    }
    if (!unknown && !Number.isSafeInteger(value)) throw new Error('WebM 元素长度过大');
    return {value, width, unknown};
}

function readElement(bytes: Uint8Array, start: number, limit = bytes.length): ElementRange {
    const id = readVint(bytes, start, true);
    const size = readVint(bytes, start + id.width);
    const dataStart = start + id.width + size.width;
    const end = size.unknown ? limit : dataStart + size.value;
    if (dataStart > limit || end > limit) throw new Error('截断的 WebM 元素');
    return {id: id.value, start, dataStart, end, unknownSize: size.unknown};
}

export function decodeEBMLElement(buffer: ArrayBuffer) {
    const bytes = new Uint8Array(buffer);
    const element = readElement(bytes, 0);
    return {
        id: element.id,
        idHex: `0x${element.id.toString(16)}`,
        dataSize: element.unknownSize ? -1 : element.end - element.dataStart,
        offset: element.end,
        data: buffer.slice(element.dataStart, element.end),
    };
}

function encodeSize(value: number): Uint8Array {
    for (let width = 1; width <= 8; width += 1) {
        if (value > 2 ** (width * 7) - 2) continue;
        const result = new Uint8Array(width);
        let remainder = value;
        for (let index = width - 1; index >= 0; index -= 1) {
            result[index] = remainder % 256;
            remainder = Math.floor(remainder / 256);
        }
        result[0] += 2 ** (8 - width);
        return result;
    }
    throw new Error('WebM 元素长度过大');
}

function concat(...parts: Uint8Array[]): Uint8Array {
    const result = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
    let offset = 0;
    for (const part of parts) { result.set(part, offset); offset += part.length; }
    return result;
}

export function setWebmDuration(buffer: ArrayBuffer, durationMs: number): ArrayBuffer {
    if (!Number.isFinite(durationMs) || durationMs <= 0) throw new Error('无效的录音时长');
    const bytes = new Uint8Array(buffer);
    const header = readElement(bytes, 0);
    if (header.id !== 0x1a45dfa3 || header.unknownSize) throw new Error('不是有效的 WebM 头');
    const segment = readElement(bytes, header.end);
    if (segment.id !== SEGMENT_ID) throw new Error('WebM 缺少 Segment');

    let info: ElementRange | undefined;
    for (let offset = segment.dataStart; offset < segment.end;) {
        const child = readElement(bytes, offset, segment.end);
        // 这里不调整已有索引的偏移；此类非流式文件交给调用方保留原文件。
        if (child.id === 0x114d9b74) throw new Error('不修正含 SeekHead 的 WebM');
        if (child.id === INFO_ID) { info = child; break; }
        offset = child.end;
    }
    if (!info || info.unknownSize) throw new Error('WebM 缺少 Info');

    let scaleNs = 1_000_000;
    const fields: Uint8Array[] = [];
    for (let offset = info.dataStart; offset < info.end;) {
        const field = readElement(bytes, offset, info.end);
        if (field.unknownSize) throw new Error('无效的 WebM Info');
        if (field.id === TIMECODE_SCALE_ID) {
            scaleNs = 0;
            for (const byte of bytes.subarray(field.dataStart, field.end)) scaleNs = scaleNs * 256 + byte;
            if (!Number.isSafeInteger(scaleNs) || scaleNs <= 0) throw new Error('无效的 WebM 时间单位');
        }
        if (field.id !== DURATION_ID) fields.push(bytes.subarray(field.start, field.end));
        offset = field.end;
    }
    const duration = new Uint8Array(8);
    new DataView(duration.buffer).setFloat64(0, durationMs * 1_000_000 / scaleNs);
    const payload = concat(...fields, new Uint8Array([0x44, 0x89, 0x88]), duration);
    const newInfo = concat(new Uint8Array([0x15, 0x49, 0xa9, 0x66]), encodeSize(payload.length), payload);
    const segmentSize = segment.end - segment.dataStart + newInfo.length - (info.end - info.start);
    const segmentHeader = segment.unknownSize
        ? bytes.subarray(segment.start, segment.dataStart)
        : concat(new Uint8Array([0x18, 0x53, 0x80, 0x67]), encodeSize(segmentSize));
    return concat(bytes.subarray(0, segment.start), segmentHeader,
        bytes.subarray(segment.dataStart, info.start), newInfo,
        bytes.subarray(info.end, segment.end), bytes.subarray(segment.end)).buffer;
}

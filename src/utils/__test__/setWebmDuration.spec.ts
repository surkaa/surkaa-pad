import {describe, expect, it} from 'vitest';
import {decodeEBMLElement, setWebmDuration} from '../setWebmDuration';

const header = [0x1a, 0x45, 0xdf, 0xa3, 0x80];
const infoId = [0x15, 0x49, 0xa9, 0x66];
const segmentId = [0x18, 0x53, 0x80, 0x67];
const cluster = [0x1f, 0x43, 0xb6, 0x75, 0x84, 0xa3, 0x82, 0x12, 0x34];
function fixture(unknown = true, fields: number[] = []) {
    const body = [...infoId, 0x80 + fields.length, ...fields, ...cluster];
    return new Uint8Array([...header, ...segmentId,
        ...(unknown ? [1, 255, 255, 255, 255, 255, 255, 255] : [0x80 + body.length]), ...body]).buffer;
}
function parse(buffer: ArrayBuffer) {
    const h = decodeEBMLElement(buffer);
    const segment = decodeEBMLElement(buffer.slice(h.offset));
    const info = decodeEBMLElement(segment.data);
    const fields = [];
    let offset = 0;
    while (offset < info.data.byteLength) {
        const field = decodeEBMLElement(info.data.slice(offset));
        fields.push(field); offset += field.offset;
    }
    return {segment, info, fields};
}
describe('MediaRecorder WebM duration', () => {
    it.each([true, false])('preserves the Segment ID, bounds and exact Cluster bytes, unknown=%s', unknown => {
        const source = fixture(unknown);
        const result = setWebmDuration(source, 123_456);
        const {segment, info, fields} = parse(result);
        expect(segment.id).toBe(0x18538067);
        expect(segment.dataSize).toBe(unknown ? -1 : segment.data.byteLength);
        expect(info.id).toBe(0x1549a966);
        expect(fields.map(f => f.id)).toEqual([0x4489]);
        expect(new DataView(fields[0].data).getFloat64(0)).toBe(123_456);
        expect([...new Uint8Array(segment.data.slice(info.offset))]).toEqual(cluster);
        expect([...new Uint8Array(source)]).toEqual([...new Uint8Array(fixture(unknown))]);
    });
    it('replaces rather than duplicates Duration and respects TimecodeScale', () => {
        const oldDuration = [...new Uint8Array(new Float64Array([0]).buffer)];
        const fields = [0x2a, 0xd7, 0xb1, 0x83, 0x1e, 0x84, 0x80, 0x44, 0x89, 0x88, ...oldDuration];
        const result = setWebmDuration(setWebmDuration(fixture(true, fields), 1000), 2000);
        const parsed = parse(result);
        expect(parsed.fields.map(f => f.id)).toEqual([0x2ad7b1, 0x4489]);
        expect(new DataView(parsed.fields[1].data).getFloat64(0)).toBe(1000);
    });
    it('handles an Info payload growing across a VINT size boundary without corrupting media', () => {
        const padding = [0xec, 0x80 + 115, ...Array(115).fill(0)];
        const result = setWebmDuration(fixture(true, padding), 3000);
        const {segment, info, fields} = parse(result);
        expect(info.data.byteLength).toBe(128);
        expect(fields[0].data.byteLength).toBe(115);
        expect([...new Uint8Array(segment.data.slice(info.offset))]).toEqual(cluster);
    });
    it.each([[], [0], header, [...header, ...segmentId, 0x90, 0]])('rejects malformed/truncated input without looping: %j', bytes => {
        expect(() => setWebmDuration(new Uint8Array(bytes).buffer, 1000)).toThrow();
    });
    it.each([0, -1, NaN, Infinity])('rejects invalid duration %s', duration => {
        expect(() => setWebmDuration(fixture(), duration)).toThrow();
    });
});

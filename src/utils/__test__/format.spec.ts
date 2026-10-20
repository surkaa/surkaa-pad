import {describe, expect, it} from 'vitest';
import {formatAttachmentTotalSize, formatBytes, formatKiB, formatTokenCount} from '../format';

describe('formatBytes', () => {
    it('preserves useful decimal precision for large attachments', () => {
        expect(formatBytes(1.56 * 1024 ** 3)).toBe('1.56 GB');
        expect(formatBytes(1.5 * 1024 ** 2)).toBe('1.5 MB');
    });

    it('omits unnecessary trailing zeroes', () => {
        expect(formatBytes(2 * 1024 ** 3)).toBe('2 GB');
        expect(formatBytes(5 * 1024 ** 2)).toBe('5 MB');
    });

    it('handles unit boundaries and missing values', () => {
        expect(formatBytes(0)).toBe('0 B');
        expect(formatBytes(1023)).toBe('1023 B');
        expect(formatBytes(1024)).toBe('1 KB');
        expect(formatBytes()).toBe('N/A');
    });
});

describe('formatAttachmentTotalSize', () => {
    it('hides totals below one megabyte', () => {
        expect(formatAttachmentTotalSize()).toBeNull();
        expect(formatAttachmentTotalSize(Number.NaN)).toBeNull();
        expect(formatAttachmentTotalSize(1024 ** 2 - 1)).toBeNull();
    });

    it('uses rounded integer units at megabyte and gigabyte boundaries', () => {
        expect(formatAttachmentTotalSize(1024 ** 2)).toBe('1 MB');
        expect(formatAttachmentTotalSize(1.5 * 1024 ** 2)).toBe('2 MB');
        expect(formatAttachmentTotalSize(1024 ** 3)).toBe('1 GB');
        expect(formatAttachmentTotalSize(1.56 * 1024 ** 3)).toBe('2 GB');
    });
});

describe('formatKiB', () => {
    it('formats kibibytes through the same precision rules', () => {
        expect(formatKiB(1536)).toBe('1.5 MB');
    });
});

describe('formatTokenCount', () => {
    it.each([
        [0, '0'], [1000, '1000'], [1024, '1K'], [32768, '32K'], [131072, '128K'],
        [1048576, '1M'], [1572864, '1.5M'], [12345, '12.06K'], [1024 ** 3, '1G'],
    ])('formats %s tokens as %s', (value, label) => {
        expect(formatTokenCount(value)).toBe(label);
    });

    it.each([12345, 1048577, 10000000])('never rounds the editable capacity %s', value => {
        expect(formatTokenCount(value, {exact: true})).toBe(String(value));
    });

    it('still abbreviates lossless editable values', () => {
        expect(formatTokenCount(1048576, {exact: true})).toBe('1M');
        expect(formatTokenCount(1280, {exact: true})).toBe('1.25K');
    });
});

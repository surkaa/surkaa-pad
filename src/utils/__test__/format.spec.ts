import {describe, expect, it} from 'vitest';
import {formatBytes, formatKiB} from '../format';

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

describe('formatKiB', () => {
    it('formats kibibytes through the same precision rules', () => {
        expect(formatKiB(1536)).toBe('1.5 MB');
    });
});

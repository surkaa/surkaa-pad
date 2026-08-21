import {describe, expect, it, vi} from 'vitest';
import {loadDiagnosticLog, type DiagnosticLogSource} from '../diagnosticLog';

function source(overrides: Partial<DiagnosticLogSource> = {}): DiagnosticLogSource {
    return {
        getAppName: async () => 'SurKaa Pad (Dev)',
        exists: async () => true,
        read: async () => new TextEncoder().encode('[ai session timing] total_ms=125\n'),
        ...overrides,
    };
}

describe('diagnostic log', () => {
    it('reads and decodes the current application log', async () => {
        const read = vi.fn(async () => new TextEncoder().encode('耗时统计 total_ms=125'));

        await expect(loadDiagnosticLog(source({read}))).resolves.toEqual({
            fileName: 'SurKaa Pad (Dev).log',
            content: '耗时统计 total_ms=125',
        });
        expect(read).toHaveBeenCalledWith('SurKaa Pad (Dev).log');
    });

    it('does not attempt to read a log that has not been created', async () => {
        const read = vi.fn<DiagnosticLogSource['read']>();

        await expect(loadDiagnosticLog(source({
            exists: async () => false,
            read,
        }))).resolves.toBeNull();
        expect(read).not.toHaveBeenCalled();
    });
});

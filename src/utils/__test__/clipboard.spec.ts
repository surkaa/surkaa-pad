// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';
import {copyTextToClipboard} from '../clipboard';

afterEach(() => {
    vi.restoreAllMocks();
});

describe('copyTextToClipboard', () => {
    it('uses the asynchronous Clipboard API when available', async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {writeText},
        });

        await copyTextToClipboard('manifest');

        expect(writeText).toHaveBeenCalledWith('manifest');
    });

    it('falls back to a temporary textarea when Clipboard API is unavailable', async () => {
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: undefined,
        });
        const execCommand = vi.fn().mockReturnValue(true);
        Object.defineProperty(document, 'execCommand', {
            configurable: true,
            value: execCommand,
        });

        await copyTextToClipboard('full manifest');

        expect(execCommand).toHaveBeenCalledWith('copy');
        expect(document.querySelector('textarea')).toBeNull();
    });

    it('falls back when the Clipboard API is present but denied', async () => {
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {writeText: vi.fn().mockRejectedValue(new Error('denied'))},
        });
        const execCommand = vi.fn().mockReturnValue(true);
        Object.defineProperty(document, 'execCommand', {
            configurable: true,
            value: execCommand,
        });

        await copyTextToClipboard('fallback manifest');

        expect(execCommand).toHaveBeenCalledWith('copy');
    });
});

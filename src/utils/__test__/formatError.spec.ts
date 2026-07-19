import {describe, expect, it} from 'vitest';
import {isNewerDiaryVersionError} from '../formatError.ts';

describe('isNewerDiaryVersionError', () => {
    it('recognizes the stable Rust error type', () => {
        expect(isNewerDiaryVersionError({
            error_type: 'diary_version_too_new',
            message: 'newer manifest',
        })).toBe(true);
    });

    it.each([
        null,
        'diary_version_too_new',
        new Error('newer manifest'),
        {error_type: 'diary'},
        {message: 'diary_version_too_new'},
    ])('does not infer the error type from %o', error => {
        expect(isNewerDiaryVersionError(error)).toBe(false);
    });
});

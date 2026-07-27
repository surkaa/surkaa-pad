import {describe, expect, it} from 'vitest';
import type {AttachmentMeta} from '../../bindings';
import {groupAttachmentsByMimeType, mimeTopLevelType} from '../attachmentGrouping';

function attachment(id: string, filename: string, mimetype: string): AttachmentMeta {
    return {
        id,
        filename,
        mimetype,
        size: 1,
        encrypted: false,
        nonce: [],
        algorithm: 'AES256-GCM_v1',
    };
}

describe('mimeTopLevelType', () => {
    it('extracts and normalizes the part before the slash', () => {
        expect(mimeTopLevelType('IMAGE/jpeg')).toBe('image');
        expect(mimeTopLevelType(' application/pdf ')).toBe('application');
    });

    it('falls back for an empty MIME type', () => {
        expect(mimeTopLevelType('')).toBe('other');
        expect(mimeTopLevelType('/octet-stream')).toBe('other');
    });
});

describe('groupAttachmentsByMimeType', () => {
    it('groups by top-level MIME type and sorts filenames lexicographically', () => {
        const input = [
            attachment('image-2', '照片B.jpg', 'image/jpeg'),
            attachment('audio-1', 'record.m4a', 'audio/mp4'),
            attachment('image-10', 'file10.jpg', 'image/png'),
            attachment('image-1', 'file2.jpg', 'image/webp'),
            attachment('image-a', '照片A.jpg', 'IMAGE/jpeg'),
        ];

        const groups = groupAttachmentsByMimeType(input);

        expect(groups.map(group => group.type)).toEqual(['audio', 'image']);
        expect(groups[1].attachments.map(item => item.filename)).toEqual([
            '照片A.jpg',
            '照片B.jpg',
            'file10.jpg',
            'file2.jpg',
        ]);
        expect(input.map(item => item.id)).toEqual([
            'image-2', 'audio-1', 'image-10', 'image-1', 'image-a',
        ]);
    });

    it('uses attachment id as a stable tie-breaker for duplicate filenames', () => {
        const groups = groupAttachmentsByMimeType([
            attachment('b', 'same.txt', 'text/plain'),
            attachment('a', 'same.txt', 'text/markdown'),
        ]);

        expect(groups[0].attachments.map(item => item.id)).toEqual(['a', 'b']);
    });
});

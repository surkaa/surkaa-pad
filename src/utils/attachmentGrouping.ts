import type {AttachmentMeta} from '../bindings';

export interface AttachmentGroup {
    type: string;
    attachments: AttachmentMeta[];
}

const filenameCollator = new Intl.Collator('zh-CN', {
    numeric: false,
    sensitivity: 'base',
});

export function mimeTopLevelType(mimetype: string): string {
    return mimetype.split('/', 1)[0].trim().toLowerCase() || 'other';
}

export function groupAttachmentsByMimeType(attachments: AttachmentMeta[]): AttachmentGroup[] {
    const grouped = new Map<string, AttachmentMeta[]>();
    for (const attachment of attachments) {
        const type = mimeTopLevelType(attachment.mimetype);
        const group = grouped.get(type) ?? [];
        group.push(attachment);
        grouped.set(type, group);
    }

    return [...grouped.entries()]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([type, group]) => ({
            type,
            attachments: [...group].sort((left, right) =>
                filenameCollator.compare(left.filename, right.filename)
                || left.id.localeCompare(right.id)
            ),
        }));
}

export function attachmentGroupIcon(type: string): string {
    switch (type) {
        case 'image': return 'image';
        case 'audio': return 'audio_file';
        case 'video': return 'video_file';
        case 'text': return 'description';
        case 'application': return 'draft';
        default: return 'attach_file';
    }
}

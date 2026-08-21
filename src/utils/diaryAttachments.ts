import type { AttachmentMeta, DiaryContent } from '../bindings'

/**
 * Collects the stable attachment IDs referenced by diary content.
 * Display metadata such as filenames deliberately does not participate in identity.
 */
export function collectReferencedAttachmentIds(content: DiaryContent): Set<string> {
  const ids = new Set<string>()

  for (const node of content.nodes) {
    switch (node.type) {
      case 'album':
        for (const attachmentId of node.attachmentIds) ids.add(attachmentId)
        break
      case 'image':
      case 'audio':
      case 'video':
      case 'file':
        ids.add(node.attachmentId)
        break
      case 'markdown':
      case 'summary':
        break
    }
  }

  return ids
}

export function findUnusedAttachments(
  content: DiaryContent,
  attachments: AttachmentMeta[],
): AttachmentMeta[] {
  const referencedIds = collectReferencedAttachmentIds(content)
  return attachments.filter(attachment => !referencedIds.has(attachment.id))
}

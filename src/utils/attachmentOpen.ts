import type {AttachmentMeta} from '../bindings';

export function isHtmlAttachment(attachment: Pick<AttachmentMeta, 'filename' | 'mimetype'>): boolean {
  const mimetype = attachment.mimetype.split(';', 1)[0].trim().toLowerCase();
  if (mimetype === 'text/html' || mimetype === 'application/xhtml+xml') return true;
  return /\.(?:html?|xhtml)$/i.test(attachment.filename.trim());
}

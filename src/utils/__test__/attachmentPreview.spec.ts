import {afterEach, describe, expect, it, vi} from 'vitest';
import type {AttachmentMeta} from '../../bindings';
import {
  attachmentPreviewKind,
  AttachmentPreviewError,
  decodeText,
  fetchAttachmentText,
  readBoundedResponseBytes,
} from '../attachmentPreview';

function attachment(filename: string, mimetype: string): AttachmentMeta {
  return {
    id: 'att-preview',
    filename,
    mimetype,
    size: 0,
    encrypted: false,
    nonce: [],
    algorithm: 'AES256-GCM_v1',
    etag: null,
    contentInfo: null,
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('attachmentPreviewKind', () => {
  it.each([
    ['document.bin', 'application/pdf', 'pdf'],
    ['README.MD', 'application/octet-stream', 'markdown'],
    ['data.bin', 'application/problem+json', 'json'],
    ['trace.LOG', 'application/octet-stream', 'text'],
    ['source.bin', 'text/x-rust', 'text'],
  ] as const)('recognizes %s as %s', (filename, mimetype, expected) => {
    expect(attachmentPreviewKind(attachment(filename, mimetype))).toBe(expected);
  });

  it('keeps executable HTML on its separate opening path', () => {
    expect(attachmentPreviewKind(attachment('page.html', 'text/plain'))).toBeNull();
    expect(attachmentPreviewKind(attachment('page.bin', 'text/html'))).toBeNull();
  });

  it('does not treat arbitrary binary files as text', () => {
    expect(attachmentPreviewKind(attachment('archive.zip', 'application/octet-stream'))).toBeNull();
  });
});

describe('bounded text loading', () => {
  it('rejects an oversized declared size before issuing a request', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);

    await expect(fetchAttachmentText('http://127.0.0.1/file', 11, undefined, 10))
      .rejects.toBeInstanceOf(AttachmentPreviewError);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('rejects an oversized content length before reading the response', async () => {
    const response = new Response('small', {headers: {'content-length': '11'}});
    await expect(readBoundedResponseBytes(response, 10))
      .rejects.toBeInstanceOf(AttachmentPreviewError);
  });

  it('rejects a streamed body that exceeds the limit', async () => {
    const response = new Response(new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2, 3]));
        controller.enqueue(new Uint8Array([4, 5, 6]));
        controller.close();
      },
    }));

    await expect(readBoundedResponseBytes(response, 5))
      .rejects.toBeInstanceOf(AttachmentPreviewError);
  });

  it('loads text through the attachment URL without using the browser cache', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response('日记文本'));
    vi.stubGlobal('fetch', fetchMock);

    await expect(fetchAttachmentText('http://127.0.0.1/file', 12))
      .resolves.toBe('日记文本');
    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1/file',
      expect.objectContaining({cache: 'no-store'}),
    );
  });
});

describe('decodeText', () => {
  it('removes an UTF-8 BOM', () => {
    expect(decodeText(new Uint8Array([0xef, 0xbb, 0xbf, 0x61]))).toBe('a');
  });

  it('decodes UTF-16 little endian text with a BOM', () => {
    expect(decodeText(new Uint8Array([0xff, 0xfe, 0x41, 0x00]))).toBe('A');
  });
});

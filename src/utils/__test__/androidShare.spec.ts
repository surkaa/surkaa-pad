import {describe, expect, it, vi} from 'vitest';
import type {DiaryContent, PendingAndroidShare} from '../../bindings';
import type {UploadedAttachment} from '../attachmentInsertion';
import {
  androidShareText,
  appendAndroidShareToDiaryContent,
  attachmentInsertionsToDiaryNodes,
  createAndroidShareResumeRefresher,
  sharedAttachmentNodeKind,
} from '../androidShare';

function batch(overrides: Partial<PendingAndroidShare> = {}): PendingAndroidShare {
  return {id: 'share-1', items: [], ...overrides};
}

function uploaded(
  nodeKind: UploadedAttachment['nodeKind'],
  attachmentId: string,
): UploadedAttachment {
  return {
    nodeKind,
    attachmentId,
    filename: `${attachmentId}.bin`,
    url: `local://${attachmentId}`,
  };
}

describe('Android share content planning', () => {
  it('refreshes immediately and retries briefly when the app resumes', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => undefined);
    const refresher = createAndroidShareResumeRefresher(refresh, [0, 100, 300]);

    refresher.trigger();
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(100);
    expect(refresh).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(200);
    expect(refresh).toHaveBeenCalledTimes(3);
    vi.useRealTimers();
  });

  it('replaces pending resume retries and cancels them when disposed', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => undefined);
    const refresher = createAndroidShareResumeRefresher(refresh, [0, 100]);

    refresher.trigger();
    refresher.trigger();
    expect(refresh).toHaveBeenCalledTimes(2);

    refresher.dispose();
    await vi.advanceTimersByTimeAsync(100);
    expect(refresh).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it('combines subject and text without duplicating an existing subject', () => {
    expect(androidShareText('标题', '正文')).toBe('标题\n\n正文');
    expect(androidShareText('标题', '标题\n正文')).toBe('标题\n正文');
    expect(androidShareText(undefined, '  正文  ')).toBe('正文');
  });

  it('converts every attachment insertion into persistent diary nodes', () => {
    expect(attachmentInsertionsToDiaryNodes([
      {type: 'image', attachmentId: 'image-1', url: ''},
      {type: 'audio', attachmentId: 'audio-1', url: ''},
      {type: 'video', attachmentId: 'video-1', url: ''},
      {type: 'file', attachmentId: 'file-1', filename: 'a.zip', url: ''},
    ])).toEqual([
      {type: 'image', attachmentId: 'image-1', size: 'normal'},
      {type: 'audio', attachmentId: 'audio-1'},
      {type: 'video', attachmentId: 'video-1'},
      {type: 'file', attachmentId: 'file-1'},
    ]);
  });

  it('appends text first and groups consecutive shared images into an album', () => {
    const content: DiaryContent = {nodes: [{type: 'markdown', text: '原正文'}]};
    const createAlbumId = vi.fn(() => 'album-1');

    const result = appendAndroidShareToDiaryContent(
      content,
      batch({subject: '分享标题', text: '分享正文'}),
      [uploaded('image', 'image-1'), uploaded('image', 'image-2'), uploaded('file', 'file-1')],
      createAlbumId,
    );

    expect(result.nodes).toEqual([
      {type: 'markdown', text: '原正文'},
      {type: 'markdown', text: '分享标题\n\n分享正文'},
      {
        type: 'album',
        id: 'album-1',
        attachmentIds: ['image-1', 'image-2'],
        displayMode: 'horizontalList',
      },
      {type: 'file', attachmentId: 'file-1'},
    ]);
    expect(content.nodes).toEqual([{type: 'markdown', text: '原正文'}]);
  });

  it('does not create an empty markdown node for file-only shares', () => {
    expect(appendAndroidShareToDiaryContent(
      {nodes: []},
      batch(),
      [uploaded('audio', 'audio-1')],
      vi.fn(),
    ).nodes).toEqual([{type: 'audio', attachmentId: 'audio-1'}]);
  });

  it('prefers the Android provider media kind over ambiguous content detection', () => {
    expect(sharedAttachmentNodeKind('audio/mp4', 'video/mp4')).toBe('audio');
    expect(sharedAttachmentNodeKind('video/mp4', 'application/octet-stream')).toBe('video');
    expect(sharedAttachmentNodeKind(null, 'image/jpeg')).toBe('image');
    expect(sharedAttachmentNodeKind('*/*', 'application/zip')).toBe('file');
  });
});

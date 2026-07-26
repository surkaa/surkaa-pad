import {describe, expect, it} from 'vitest';
import {countAttachmentTypes} from '../attachmentTypeCounts.ts';

describe('countAttachmentTypes', () => {
  it('分别统计图片、音频、视频和文件', () => {
    expect(countAttachmentTypes([
      {mimetype: 'image/jpeg'},
      {mimetype: 'IMAGE/PNG'},
      {mimetype: 'audio/mpeg'},
      {mimetype: 'video/mp4'},
      {mimetype: 'application/pdf'},
      {mimetype: 'text/plain'},
    ])).toEqual({image: 2, audio: 1, video: 1, file: 2});
  });

  it('将未知或不完整的 MIME 类型归为文件', () => {
    expect(countAttachmentTypes([
      {mimetype: ''},
      {mimetype: 'unknown'},
      {mimetype: 'image'},
    ])).toEqual({image: 0, audio: 0, video: 0, file: 3});
  });

  it('空附件列表的各项统计均为零', () => {
    expect(countAttachmentTypes([])).toEqual({image: 0, audio: 0, video: 0, file: 0});
  });
});

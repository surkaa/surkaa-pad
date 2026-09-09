import {describe, expect, it} from 'vitest';
import {isHtmlAttachment} from '../attachmentOpen';

describe('isHtmlAttachment', () => {
  it.each([
    ['page.bin', 'text/html; charset=utf-8'],
    ['page.bin', 'application/xhtml+xml'],
    ['page.HTML', 'application/octet-stream'],
    ['page.xhtml', 'text/plain'],
  ])('recognizes %s (%s)', (filename, mimetype) => {
    expect(isHtmlAttachment({filename, mimetype})).toBe(true);
  });

  it('does not treat unrelated text and files as HTML', () => {
    expect(isHtmlAttachment({filename: 'notes.txt', mimetype: 'text/plain'})).toBe(false);
    expect(isHtmlAttachment({filename: 'archive.zip', mimetype: 'application/zip'})).toBe(false);
  });
});

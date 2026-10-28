import {describe, expect, it} from 'vitest';
import {diaryContentToMarkdown} from '../diaryMarkdown';

describe('diary Markdown clipboard content', () => {
  it('preserves Markdown verbatim, including lists and tables', () => {
    const text = '# 标题\n\n- 父级\n  - 子级\n\n| A | B |\n| --- | --- |\n| 1 | 2 |';
    expect(diaryContentToMarkdown({nodes: [{type: 'markdown', text}]})).toBe(text);
    expect(diaryContentToMarkdown({nodes: []})).toBe('');
  });

  it('copies every attachment in document order using relative, encoded links', () => {
    const result = diaryContentToMarkdown({nodes: [
      {type: 'markdown', text: '开头'},
      {type: 'image', attachmentId: 'image', size: 'normal'},
      {type: 'video', attachmentId: 'video'},
      {type: 'audio', attachmentId: 'audio'},
      {type: 'file', attachmentId: 'file'},
      {type: 'album', id: 'album', attachmentIds: ['first', 'second'], displayMode: 'horizontalList'},
      {type: 'markdown', text: '结尾'},
    ]}, [
      {id: 'image', filename: '图片[1].jpg'},
      {id: 'file', filename: '报告 #最终.pdf'},
    ]);
    expect(result).toBe([
      '开头',
      '![图片\\[1\\].jpg](attachments/%E5%9B%BE%E7%89%87%5B1%5D.jpg)',
      '[video](attachments/video)', '[audio](attachments/audio)',
      '[报告 #最终.pdf](attachments/%E6%8A%A5%E5%91%8A%20%23%E6%9C%80%E7%BB%88.pdf)',
      '![first](attachments/first)', '![second](attachments/second)', '结尾',
    ].join('\n\n'));
    expect(result).not.toMatch(/localhost|127\.0\.0\.1|token=/);
  });

  it('escapes the Summary title while preserving its Markdown body verbatim', () => {
    expect(diaryContentToMarkdown({nodes: [
      {type: 'summary', summary: '<标题>&', content: [{type: 'markdown', text: '## 内容\n\n**加粗**\n\n```html\n<div>代码</div>\n```'}]},
    ]})).toBe('<details>\n<summary>&lt;标题&gt;&amp;</summary>\n\n'
      + '## 内容\n\n**加粗**\n\n```html\n<div>代码</div>\n```\n\n</details>');
  });

  it('includes location coordinates and optional accuracy', () => {
    const location = {
      coordinateSystem: 'wgs84' as const, latitude: 23.1, longitude: 113.2,
      capturedAt: 1000, placeName: '地点', horizontalAccuracyMeters: 12.5,
      altitudeMeters: null, verticalAccuracyMeters: null,
    };
    expect(diaryContentToMarkdown({nodes: [{type: 'location', location}]}))
      .toBe('📍 地点：23.1, 113.2（WGS-84） · 精度约 ±13 米');
    expect(diaryContentToMarkdown({nodes: [{type: 'location', location: {
      ...location, placeName: null, horizontalAccuracyMeters: null,
    }}]})).toBe('📍 未命名地点：23.1, 113.2（WGS-84）');
  });
});

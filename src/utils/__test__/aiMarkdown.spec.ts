import {describe, expect, it} from 'vitest';
import {renderAiMarkdown} from '../aiMarkdown';

describe('renderAiMarkdown', () => {
  it('renders common answer formatting including tables', () => {
    const html = renderAiMarkdown([
      '## 标题',
      '',
      '**重点**',
      '',
      '| 项目 | 结果 |',
      '| --- | --- |',
      '| 搜索 | 正常 |',
    ].join('\n'));

    expect(html).toContain('<h2>标题</h2>');
    expect(html).toContain('<strong>重点</strong>');
    expect(html).toContain('<table>');
    expect(html).toContain('<td>正常</td>');
  });

  it('escapes raw HTML and rejects dangerous links', () => {
    const html = renderAiMarkdown([
      '<script>alert(1)</script>',
      '',
      '[危险链接](javascript:alert(1))',
    ].join('\n'));

    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
    expect(html).not.toContain('<script>');
    expect(html).not.toContain('href="javascript:');
  });

  it('does not create network-loading image elements', () => {
    const html = renderAiMarkdown('![测试图](https://example.com/private.png)');

    expect(html).not.toContain('<img');
    expect(html).not.toContain('private.png');
    expect(html).toContain('[图片：测试图]');
  });
});

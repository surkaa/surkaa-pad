// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, reactive, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import type {AttachmentMeta} from '../../../bindings';
import Component from '../AttachmentBatchDownloadDialog.vue';

let app: App | undefined;
const download = vi.fn();
function attachment(id: string, filename: string): AttachmentMeta {
  return {id, filename, mimetype: 'application/octet-stream', size: 1024, encrypted: true,
    nonce: [], algorithm: 'AES-256-CTR', etag: null, contentInfo: null};
}
async function settle() { await nextTick(); await vi.advanceTimersByTimeAsync(400); await nextTick(); }
async function mount(dark: boolean) {
  const props = reactive({modelValue: true, attachments: [attachment('a', 'REPORT.pdf'), attachment('b', '照片.jpg'),
    attachment('c', 'Report.json')], initialAttachmentId: 'b'});
  const host = document.createElement('div'); document.body.append(host);
  app = createApp({render: () => h(Component, {...props, 'onUpdate:modelValue': value => { props.modelValue = value; }, onDownload: download})});
  app.use(Quasar); Dark.set(dark); app.mount(host); await settle();
  return props;
}
function button(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.trim() === label)!;
}
async function search(value: string) {
  const input = document.querySelector<HTMLInputElement>('.batch-filter input')!;
  input.value = value; input.dispatchEvent(new Event('input', {bubbles: true})); await settle();
}
beforeEach(() => { vi.useFakeTimers(); download.mockClear(); });
afterEach(() => { app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = ''; vi.useRealTimers(); });

describe.each([false, true])('批量附件选择（深色=%s）', dark => {
  it('预选右键附件，不修改附件内容，按钮始终位于列表滚动区之外', async () => {
    await mount(dark);
    expect(document.querySelector('[data-attachment-id="b"] [role="checkbox"]')?.getAttribute('aria-checked')).toBe('true');
    expect(document.querySelector('[data-attachment-id="a"] [role="checkbox"]')?.getAttribute('aria-checked')).toBe('false');
    expect(document.querySelector('.q-field')?.classList.contains('q-field--dark')).toBe(dark);
    expect(document.querySelector('[role="checkbox"]')?.classList.contains('q-checkbox--dark')).toBe(dark);
    expect(button('选择文件夹并下载').closest('.batch-files')).toBeNull();
    button('选择文件夹并下载').click(); await settle();
    expect(download).toHaveBeenCalledExactlyOnceWith(['b']);
  });

  it('文件名检索不区分大小写，全选只影响当前结果，导出不遗漏筛选前选择', async () => {
    await mount(dark); await search('report');
    expect(document.querySelectorAll('.batch-file')).toHaveLength(2);
    button('全选当前结果').click(); await settle();
    expect(document.querySelector('.batch-actions')?.textContent).toContain('已选 3 个');
    button('取消选择当前结果').click(); await settle();
    expect(document.querySelector('.batch-actions')?.textContent).toContain('已选 1 个');
    button('全选当前结果').click(); await settle();
    button('选择文件夹并下载').click(); await settle();
    expect(download).toHaveBeenCalledExactlyOnceWith(['a', 'b', 'c']);
  });

  it('附件变动时丢弃失效选择，重新打开时清空旧筛选并重新预选', async () => {
    const props = await mount(dark);
    props.attachments = [attachment('a', 'REPORT.pdf')]; await settle();
    expect(button('选择文件夹并下载').disabled).toBe(true);
    await search('不存在'); expect(document.querySelector('.batch-empty')?.textContent).toBe('没有匹配的附件');
    props.modelValue = false; await settle(); props.initialAttachmentId = 'a'; props.modelValue = true; await settle();
    expect(document.querySelector<HTMLInputElement>('.batch-filter input')?.value).toBe('');
    button('选择文件夹并下载').click(); await settle();
    expect(download).toHaveBeenCalledExactlyOnceWith(['a']);
  });
});

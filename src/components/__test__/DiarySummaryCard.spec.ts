// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, reactive, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import Component from '../DiarySummaryCard.vue';
import type {DiarySummary} from '../../bindings';

let app: App | undefined;
let host: HTMLElement;
const observe = vi.fn();
const disconnect = vi.fn();
let onIntersection: IntersectionObserverCallback;
const summary: DiarySummary = {
  id: '8215021834823', title: '置顶日记标题', created: 1_700_000_000_000, updated: 1_700_000_060_000,
  attachmentCount: 21,
  attachmentCounts: {image: 12, audio: 2, video: 3, file: 4},
  encryptedAttachmentCounts: {image: 6, audio: 2, video: 0, file: 1},
  attachmentTotalSize: 8 * 1024 ** 2,
};

async function mount(diary: DiarySummary | null = summary, pinned = true) {
  host = document.createElement('div'); document.body.append(host);
  const visible = vi.fn();
  const click = vi.fn();
  const props = reactive({diary, pinned, onVisible: visible, onClick: click});
  // 使用响应式 props 覆盖骨架加载完成及取消置顶时的真实组件切换。
  app = createApp({render: () => h(Component, props)});
  app.use(Quasar);
  app.mount(host);
  await nextTick();
  return {props, visible, click};
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('IntersectionObserver', class {
    observe = observe;
    disconnect = disconnect;
    constructor(callback: IntersectionObserverCallback) { onIntersection = callback; }
  });
});
afterEach(() => {
  app?.unmount(); app = undefined;
  document.body.innerHTML = '';
  Dark.set(false);
  vi.unstubAllGlobals();
});

describe.each([false, true])('日记摘要卡片（深色=%s）', dark => {
  it('置顶卡片只保留标题、附件类型统计和总大小', async () => {
    Dark.set(dark); await mount();
    expect(host.querySelector('.diary-card--pinned')).not.toBeNull();
    expect(host.querySelector('.preview-title')?.textContent).toBe(summary.title);
    expect(host.querySelector('.pinned-icon')).not.toBeNull();
    expect(host.querySelector('.card-header')).toBeNull();
    expect(host.querySelector('.diary-id')).toBeNull();
    expect(host.querySelector('.date-primary')).toBeNull();
    expect(host.querySelector('.date-updated')).toBeNull();
    expect(host.textContent).not.toContain(summary.id);
    expect(Array.from(host.querySelectorAll('.attachment-count > span:last-child'), item => item.textContent)).toEqual(['×12', '×2', '×3', '×4']);
    expect(host.querySelector('.attachment-total-size')?.textContent).toContain('8 MB');
    expect(host.querySelector('.attachment-count[title="图片"] .attachment-encryption-icon')?.getAttribute('style'))
      .toContain('--encrypted-fill: 50%');
  });

  it('普通卡片仍显示时间、ID 和原有附件总览', async () => {
    Dark.set(dark); await mount(summary, false);
    expect(host.querySelector('.diary-card--pinned')).toBeNull();
    expect(host.querySelector('.card-header')).not.toBeNull();
    expect(host.querySelector('.date-primary')?.textContent?.trim()).not.toBe('');
    expect(host.querySelector('.date-updated')).not.toBeNull();
    expect(host.querySelector('.diary-id')?.textContent).toContain(summary.id);
    expect(host.querySelector('.attachment-badge')?.textContent).toContain('8 MB');
    expect(host.querySelectorAll('.attachment-count')).toHaveLength(4);
  });

  it('没有附件时不留空白底栏，缺少标题时保留占位', async () => {
    Dark.set(dark);
    await mount({...summary, title: '', attachmentCounts: {image: 0, audio: 0, video: 0, file: 0}, attachmentTotalSize: 0});
    expect(host.querySelector('.preview-title')?.textContent).toBe('（无内容预览）');
    expect(host.querySelector('.card-footer')).toBeNull();
    expect(host.querySelector('.card-header')).toBeNull();
  });

  it('置顶骨架加载完成后仍正确更新，取消置顶可恢复完整卡片', async () => {
    Dark.set(dark);
    const {props, visible, click} = await mount(null);
    expect(host.querySelector('.skeleton-mode.diary-card--pinned')).not.toBeNull();
    expect(host.querySelector('.card-header')).toBeNull();
    expect(observe).toHaveBeenCalledTimes(1);
    onIntersection([{isIntersecting: true} as IntersectionObserverEntry], {} as IntersectionObserver);
    expect(visible).toHaveBeenCalledTimes(1);
    expect(disconnect).toHaveBeenCalledTimes(1);
    props.diary = summary;
    await nextTick();
    expect(host.querySelector('.skeleton-mode')).toBeNull();
    expect(host.querySelector('.preview-title')?.textContent).toBe(summary.title);
    (host.querySelector('.diary-card') as HTMLElement).click();
    expect(click).toHaveBeenCalledTimes(1);
    props.pinned = false;
    await nextTick();
    expect(host.querySelector('.diary-card--pinned')).toBeNull();
    expect(host.querySelector('.diary-id')?.textContent).toContain(summary.id);
  });
});

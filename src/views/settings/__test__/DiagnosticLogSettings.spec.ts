// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, nextTick, type App} from 'vue';
import {Quasar, Dark, Notify, Screen} from 'quasar';
import Component from '../DiagnosticLogSettings.vue';
import {reportFixture} from '../../../utils/__test__/fixtures/vaultKeyChangeReport';
import type {KeyChangeReport} from '../../../bindings';

const mocks = vi.hoisted(() => ({getReport: vi.fn(), loadLog: vi.fn(), about: vi.fn(), copy: vi.fn()}));
vi.mock('../../../utils/api', () => ({default: {cmdGetVaultKeyChangeReport: mocks.getReport}}));
vi.mock('../../../utils/diagnosticLog', () => ({loadDiagnosticLog: mocks.loadLog}));
vi.mock('../../../utils/aboutInfo', () => ({loadAboutInfo: mocks.about}));
vi.mock('../../../utils/clipboard', () => ({copyTextToClipboard: mocks.copy}));

let app: App | undefined;
const initiallySmall = Screen.lt.sm;
async function mount(dark = false, mobile = false) {
  const host = document.createElement('div'); document.body.append(host);
  app = createApp(Component); app.use(Quasar, {plugins: {Notify}});
  Screen.lt.sm = mobile; app.mount(host);
  Dark.set(dark);
  host.querySelector<HTMLElement>('.q-item')!.click();
  await vi.advanceTimersByTimeAsync(500); await nextTick();
}
function button(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.includes(label))!;
}
async function selectTab(label: string) {
  [...document.querySelectorAll<HTMLElement>('[role="tab"]')].find(t => t.textContent?.includes(label))!.click();
  await nextTick();
}

describe('diagnostic performance report', () => {
  beforeEach(() => {
    vi.useFakeTimers(); mocks.copy.mockReset().mockResolvedValue(undefined);
    mocks.getReport.mockReset().mockResolvedValue(reportFixture());
    mocks.loadLog.mockReset().mockResolvedValue({fileName: 'test.log', content: 'example runtime log'});
    mocks.about.mockReset().mockResolvedValue({appVersion: '0.8.11', gitCommit: '12345678'});
  });
  afterEach(() => {
    app?.unmount(); app = undefined; Dark.set(false); Screen.lt.sm = initiallySmall; document.body.innerHTML = ''; vi.useRealTimers();
  });

  it.each([false, true])('shows a separately scrollable report and copies complete JSON (dark=%s)', async dark => {
    await mount(dark);
    expect(document.body.textContent).toContain('改密已完成 · 5 / 5 个对象');
    expect(document.body.textContent).toContain('2 分 0 秒');
    expect(document.querySelector('.json-tree-viewer')).not.toBeNull();
    expect(document.querySelector('.diagnostic-content')).toBeNull();
    expect(document.querySelector('.diagnostic-report-content .is-content-height')).not.toBeNull();
    expect(document.querySelector('.vjs-tree')?.classList.contains('dark')).toBe(dark);
    button('复制改密报告').click(); await vi.advanceTimersByTimeAsync(0);
    const copied = JSON.parse(mocks.copy.mock.calls[0][0]);
    expect(copied.measurements).toEqual(reportFixture());
    expect(copied.application.version).toBe('0.8.11');
    expect(copied.notes.join('')).toContain('不是 HTTP 请求数');
    await selectTab('运行日志');
    button('复制全部').click(); await vi.advanceTimersByTimeAsync(0);
    expect(mocks.copy).toHaveBeenLastCalledWith('example runtime log');
  });

  it('keeps the report accessible when the runtime log or build metadata cannot be read', async () => {
    mocks.loadLog.mockRejectedValue(new Error('log unavailable')); mocks.about.mockRejectedValue(new Error('metadata unavailable'));
    await mount();
    expect(button('复制改密报告').disabled).toBe(false);
    button('复制改密报告').click(); await vi.advanceTimersByTimeAsync(0);
    expect(JSON.parse(mocks.copy.mock.calls[0][0]).application).toEqual({version: '未知', gitCommit: '未知'});
    await selectTab('运行日志'); expect(document.body.textContent).toContain('log unavailable');
  });

  it('handles empty and failed reports without preventing log viewing', async () => {
    mocks.getReport.mockResolvedValue(null);
    await mount(); await selectTab('改密统计');
    expect(document.body.textContent).toContain('暂无改密统计');
    expect(button('复制改密报告').disabled).toBe(true);
    mocks.getReport.mockRejectedValue(new Error('report unavailable'));
    button('刷新').click(); await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.body.textContent).toContain('report unavailable');
    await selectTab('运行日志'); expect(document.body.textContent).toContain('example runtime log');
  });

  it('marks incomplete history and re-reads measurements on refresh', async () => {
    const report: KeyChangeReport = {...reportFixture(), outcome: 'paused', incompleteHistory: true};
    mocks.getReport.mockResolvedValue(report); await mount();
    expect(document.body.textContent).toContain('已暂停');
    expect(document.body.textContent).toContain('此报告只包含已保留的部分');
    mocks.getReport.mockResolvedValue(reportFixture());
    button('刷新').click(); await vi.advanceTimersByTimeAsync(0); await nextTick();
    expect(document.body.textContent).toContain('改密已完成');
    expect(document.body.textContent).not.toContain('此报告只包含已保留的部分');
  });

  it.each([false, true])('keeps report and log content when toggling fullscreen (dark=%s)', async dark => {
    await mount(dark);
    const tree = document.querySelector('.json-tree-viewer');
    document.querySelector<HTMLButtonElement>('[aria-label="全屏查看"]')!.click(); await nextTick();
    expect(document.querySelector('.diagnostic-dialog.is-maximized')).not.toBeNull();
    expect(document.querySelector('.json-tree-viewer')).toBe(tree);
    await selectTab('运行日志');
    expect(document.body.textContent).toContain('example runtime log');
    document.querySelector<HTMLButtonElement>('[aria-label="退出全屏"]')!.click(); await nextTick();
    expect(document.querySelector('.diagnostic-dialog.is-maximized')).toBeNull();
    expect(mocks.getReport).toHaveBeenCalledTimes(1);
    expect(mocks.loadLog).toHaveBeenCalledTimes(1);
    document.querySelector<HTMLButtonElement>('[aria-label="全屏查看"]')!.click(); await nextTick();
    document.querySelector<HTMLButtonElement>('[aria-label="关闭诊断信息弹窗"]')!.click();
    await vi.advanceTimersByTimeAsync(500);
    document.querySelector<HTMLElement>('.q-item')!.click();
    await vi.advanceTimersByTimeAsync(500); await nextTick();
    expect(document.querySelector('.diagnostic-dialog.is-maximized')).toBeNull();
  });

  it('uses fullscreen automatically on small screens', async () => {
    await mount(true, true);
    expect(document.querySelector('.diagnostic-dialog.is-maximized')).not.toBeNull();
    expect(document.querySelector('.q-dialog__inner--maximized')).not.toBeNull();
    expect(document.querySelector('[aria-label="退出全屏"]')).toBeNull();
  });
});

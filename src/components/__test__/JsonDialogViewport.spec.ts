// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App, type Component} from 'vue';
import {Dark, Notify, Quasar, Screen} from 'quasar';
import JsonSourceDialog from '../JsonSourceDialog.vue';
import AiSessionPersistenceDialog from '../../views/ai-assistant/AiSessionPersistenceDialog.vue';
import {copyTextToClipboard} from '../../utils/clipboard';

vi.mock('../../utils/clipboard', () => ({copyTextToClipboard: vi.fn().mockResolvedValue(undefined)}));

const initiallySmall = Screen.lt.sm;
const source = {nested: '{"original":"unchanged"}'};
const cases = [
  {name: 'shared JSON dialog', component: JsonSourceDialog, card: '.json-source-card', props: {title: 'JSON 内容', source}},
  {name: 'AI persistence dialog', component: AiSessionPersistenceDialog, card: '.session-persistence-card', props: {
    meta: {version: 2, id: '123', title: 'Test', aiTitle: null, createdAt: 0, updatedAt: 0, committedMessageCount: 2},
    messages: [], totalMessageCount: 2, loadingMeta: false, loadingMessages: false,
    messageError: null, nextMessageBatchSize: 2,
  }},
];
let app: App | undefined;

async function settle() {
  await vi.advanceTimersByTimeAsync(400);
  await nextTick();
}

function action(label: string) {
  return document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`);
}

async function mount(component: Component, props: object, dark = false, mobile = false) {
  const visible = ref(true);
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp(defineComponent({setup: () => () => h(component, {
    ...props, modelValue: visible.value, 'onUpdate:modelValue': (value: boolean) => { visible.value = value; },
  })}));
  app.use(Quasar, {plugins: {Notify}});
  Screen.lt.sm = mobile;
  Dark.set(dark);
  app.mount(host);
  await settle();
  return visible;
}

beforeEach(() => { vi.useFakeTimers(); vi.mocked(copyTextToClipboard).mockClear(); });
afterEach(() => {
  app?.unmount(); app = undefined;
  Dark.set(false); Screen.lt.sm = initiallySmall;
  document.body.innerHTML = ''; vi.useRealTimers();
});

describe.each(cases)('$name viewport', ({component, card, props}) => {
  it.each([false, true])('toggles fullscreen without remounting JSON in either theme (dark=%s)', async dark => {
    await mount(component, props, dark);
    const tree = document.querySelector('.json-tree-viewer');
    expect(tree).not.toBeNull();
    expect(document.querySelector('.vjs-tree')?.classList.contains('dark')).toBe(dark);
    expect(document.querySelector(`${card}.is-maximized`)).toBeNull();
    action('全屏查看')!.click(); await settle();
    expect(document.querySelector(`${card}.is-maximized`)).not.toBeNull();
    expect(document.querySelector('.q-dialog__inner--maximized')).not.toBeNull();
    expect(document.querySelector('.json-tree-viewer')).toBe(tree);
    action('退出全屏')!.click(); await settle();
    expect(document.querySelector(`${card}.is-maximized`)).toBeNull();
    expect(document.querySelector('.json-tree-viewer')).toBe(tree);
  });

  it('automatically uses fullscreen on small screens and responds to screen changes', async () => {
    await mount(component, props, true, true);
    expect(document.querySelector(`${card}.is-maximized`)).not.toBeNull();
    expect(action('退出全屏')).toBeNull();
    expect(action('全屏查看')).toBeNull();
    Screen.lt.sm = false; await settle();
    expect(document.querySelector(`${card}.is-maximized`)).toBeNull();
    expect(action('全屏查看')).not.toBeNull();
  });

  it('resets manual fullscreen after closing without closing the dialog on toggle', async () => {
    const visible = await mount(component, props);
    action('全屏查看')!.click(); await settle();
    expect(visible.value).toBe(true);
    visible.value = false; await settle();
    visible.value = true; await settle();
    expect(document.querySelector(`${card}.is-maximized`)).toBeNull();
    expect(action('全屏查看')).not.toBeNull();
  });
});

it('keeps copying the original JSON after fullscreen and JSON string display changes', async () => {
  await mount(JsonSourceDialog, {title: 'JSON 内容', source});
  const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>('button')]
    .find(element => element.textContent?.includes(text))!;
  button('显示原始字符串').click(); await settle();
  action('全屏查看')!.click(); await settle();
  expect(button('解析 JSON 字符串')).toBeDefined();
  button('复制 JSON').click(); await settle();
  expect(copyTextToClipboard).toHaveBeenCalledWith(JSON.stringify(source, null, 2));
});

it('allows fullscreen while JSON is still loading', async () => {
  await mount(JsonSourceDialog, {title: 'JSON 内容', loading: true});
  action('全屏查看')!.click(); await settle();
  expect(document.querySelector('.json-source-card.is-maximized')).not.toBeNull();
  expect(document.body.textContent).toContain('正在读取 JSON');
});

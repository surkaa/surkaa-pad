// @vitest-environment happy-dom
import {afterEach, describe, expect, it, vi} from 'vitest';

// 由 Vitest 的 Node 宿主读文件，不向前端 DOM 类型注入 Node 全局计时器类型。
const {readFileSync} = await vi.importActual<{
  readFileSync(path: string, encoding: 'utf8'): string;
}>('node:fs');
const stylesheet = readFileSync('src/assets/style.css', 'utf8');
afterEach(() => { document.head.innerHTML = ''; document.body.innerHTML = ''; document.body.className = ''; document.documentElement.removeAttribute('style'); });
describe('Teleport popup CSS precedence', () => {
  it.each([false, true])('wins over a later Quasar menu background and follows theme variables (dark=%s)', dark => {
    const theme = document.createElement('style'); theme.textContent = stylesheet;
    // 真实 main.ts 中 Quasar CSS 在项目主题 CSS 之后；类名存在不代表背景正确。
    const defaults = document.createElement('style'); defaults.textContent = '.q-menu { background-color: white; color: black; }';
    document.head.append(theme, defaults);
    document.body.classList.toggle('body--dark', dark);
    // happy-dom 的嵌套 CSS 自定义属性继承不完整，这里显式模拟活动主题变量。
    // 真实浏览器另外检查 body--dark 切换后的 computed style。
    document.documentElement.style.setProperty('--pad-bg-color-200', dark ? '#262E38' : '#F7F5F2');
    document.documentElement.style.setProperty('--pad-text-color-100', dark ? '#FDF5E6' : '#333333');
    const popup = document.createElement('div'); popup.className = 'q-menu'; document.body.append(popup);
    const style = getComputedStyle(popup);
    expect(style.backgroundColor).toBe(dark ? '#262E38' : '#F7F5F2');
    expect(style.color).toBe(dark ? '#FDF5E6' : '#333333');
  });
});

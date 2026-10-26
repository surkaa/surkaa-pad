import {describe, expect, it} from 'vitest';
import {desktopDevEnvironment} from '../tauri-msi-dev.mjs';

describe('Windows 开发启动环境', () => {
  it('每次启动都设置工作区之外的构建目录，不修改调用者环境', () => {
    const original = {LOCALAPPDATA: 'C:\\Users\\Tester\\AppData\\Local', OTHER: 'keep'};
    const expected = {...original, VITE_PORT: '25173', CARGO_TARGET_DIR: 'C:\\Users\\Tester\\AppData\\Local\\SurKaaPadBuild'};
    expect(desktopDevEnvironment(original, 'win32')).toEqual(expected);
    expect(desktopDevEnvironment(original, 'win32')).toEqual(expected);
    expect(original).not.toHaveProperty('CARGO_TARGET_DIR');
    expect(original).not.toHaveProperty('VITE_PORT');
  });

  it('尊重调用者显式指定的构建目录，包括包含空格的路径', () => {
    expect(desktopDevEnvironment({CARGO_TARGET_DIR: 'D:\\My Builds\\pad'}, 'win32'))
      .toMatchObject({CARGO_TARGET_DIR: 'D:\\My Builds\\pad', VITE_PORT: '25173'});
  });

  it('非 Windows 平台不改构建目录', () => {
    expect(desktopDevEnvironment({}, 'linux')).toEqual({VITE_PORT: '25173'});
    expect(desktopDevEnvironment({CARGO_TARGET_DIR: '/tmp/pad'}, 'darwin'))
      .toEqual({CARGO_TARGET_DIR: '/tmp/pad', VITE_PORT: '25173'});
  });

  it('无法确定用户目录时明确报错，不悄悄退回带有低完整性标签的目录', () => {
    expect(() => desktopDevEnvironment({}, 'win32')).toThrow('LOCALAPPDATA');
    expect(desktopDevEnvironment({LOCALAPPDATA: 'C:\\Local', CARGO_TARGET_DIR: ' '}, 'win32'))
      .toMatchObject({CARGO_TARGET_DIR: 'C:\\Local\\SurKaaPadBuild'});
  });
});

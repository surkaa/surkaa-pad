import {createRequire} from 'node:module';
import {resolve, win32} from 'node:path';
import {fileURLToPath} from 'node:url';

export function desktopDevEnvironment(environment, platform) {
  const result = {...environment, VITE_PORT: '25173'};
  if (platform === 'win32' && !result.CARGO_TARGET_DIR?.trim()) {
    if (!result.LOCALAPPDATA?.trim()) {
      throw new Error('缺少 LOCALAPPDATA，无法确定 Windows 开发构建目录；请显式设置 CARGO_TARGET_DIR。');
    }
    // 工作区可能继承 Low 完整性标签；开发 exe 放在普通用户目录，避免启动时降权。
    // 仅影响本次 Windows 开发命令，不写用户级环境变量、不更改目录权限。
    result.CARGO_TARGET_DIR = win32.join(result.LOCALAPPDATA, 'SurKaaPadBuild');
  }
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    Object.assign(process.env, desktopDevEnvironment(process.env, process.platform));
    const require = createRequire(import.meta.url);
    // 与官方 tauri.js 使用同一入口，保留 CLI 自身的信号处理和退出流程。
    const {run} = require('@tauri-apps/cli');
    await run(['dev', '--config', './src-tauri/tauri.dev.json', ...process.argv.slice(2)], 'pnpm tauri:msi:dev');
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

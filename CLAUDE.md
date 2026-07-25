# CLAUDE.md

## 工作规范

### 编码规范

- 正确性和清晰性优先于速度和效率。先保证逻辑正确，再优化性能。

### 测试要求

- **能写测试就写测试**。任何涉及逻辑判断、边界条件、数据处理、加解密、缓存校验的代码都应该有对应的测试用例。
- 前端用 Vitest + happy-dom（DOM 环境），测试文件放在 `__test__/` 目录下。需 DOM 的测试在文件顶部加 `// @vitest-environment happy-dom` 指令。
- 后端 Rust 测试放在各模块的 `*_tests.rs` 文件中，或通过 `#[cfg(test)]` 内嵌在模块内。
- 写完测试后务必跑一遍确认通过：前端 `pnpm vitest -- run`，后端 `cargo test`。
- 部分 Rust 测试需要阿里云 OSS 凭证，需在 `src-tauri/` 下配置 `.env` 文件（`ALIYUN_KEY`、`ALIYUN_SECRET`、`ALIYUN_BUCKET_NAME`、`ALIYUN_ENDPOINT`、`ALIYUN_REGION`），并使用 `serial_test` 控制互斥执行。

### Rust 代码检查

- 所有 Rust 修改完成后，运行 `cargo clippy` 检查并修复警告。
- 不要在代码中残留 `#[allow(dead_code)]` 或 `#[allow(unused)]` 标注，除非有明确的阶段性保留理由。

### 开发启动与 bindings 生成

- **Windows 开发版统一从项目根目录运行 `pnpm tauri:msi:dev`**。该命令会按项目配置启动 Vite 和正确的 Tauri bin；不要用 `cargo run` 直接启动应用，也不要用未指定项目配置的通用 `tauri dev`。
- Rust 端新增或修改 Tauri command、Specta 类型后，使用 `pnpm tauri:msi:dev` 启动一次 Windows 调试版，让 tauri-specta 自动更新 `src/bindings.ts`。生成时弹出应用 GUI 属于正常现象，确认 bindings 已更新后可关闭应用。
- Android 开发版使用 `pnpm tauri:android:dev`。`cargo test`、`cargo clippy` 等纯 Rust 检查仍在 `src-tauri/` 目录执行。

### 分支与提交

- **大修改开新分支**。任何涉及多文件、新功能、重构的修改都应在独立 feature 分支上进行，不要在 `master` 上直接大量修改。
- **小步提交**，不要让未提交的修改大量堆积。每完成一个可独立工作的逻辑单元就提交一次。
- 提交信息遵循已有风格：`<type>: <描述>`，如 `feat:`、`fix:`、`refactor:`、`perf:`、`docs:`、`test:`。

### 不要做的事

- **严禁手动编辑 `src/bindings.ts`**。该文件由 tauri-specta 在 Windows 调试构建时自动生成，手动修改会在下次构建时被覆盖。新增或修改命令签名应通过 Rust 端的 `#[tauri::command]` 和 `#[specta]` 导出。
- 不要随意引入新的重量级依赖，优先复用现有 crate/package。
- 不要在 PR/提交中包含 `.env`、`node_modules`、`target` 等敏感或构建产物文件。

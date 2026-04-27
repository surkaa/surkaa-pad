# AGENT.md

> 生成时间：2026-04-27 | Git: `30d3ebd`

## 项目概述

SurKaa Pad 是一款基于 Tauri 2 的端到端加密日记应用。前端 Vue 3 + TypeScript + Quasar，后端 Rust。日记正文用 AES-256-GCM 加密，附件用 AES-256-CTR 流式加密，密钥通过 Argon2id 从主密码派生，加密后同步至阿里云 OSS。

## 常用命令

```bash
# 前端
pnpm install                    # 安装依赖
pnpm dev                        # Vite 开发服务器 (端口 5173)
pnpm build                      # 类型检查 + 构建
pnpm tauri:msi:dev              # Tauri 桌面开发模式
pnpm tauri:msi:build            # Tauri 桌面生产构建
pnpm tauri:android:dev          # Tauri Android 开发模式
pnpm tauri:android:build        # Tauri Android APK 构建

# 后端 (Rust, 在 src-tauri 目录下执行)
cargo build
cargo test                      # 运行全部 Rust 测试
cargo test -- <测试名>           # 运行单个测试
cargo clippy                    # Rust 代码检查

# 前端测试
pnpm vitest                     # Vitest 测试（监视模式）
pnpm vitest -- run              # 单次运行
pnpm tsc                        # 仅类型检查 (vue-tsc --noEmit)
```

## 架构

### 前端 (`src/`)

- **路由** (`src/router/index.ts`): Hash 模式，带 keep-alive 页面缓存管理。`/` (Unlock) → `/diary-list`, `/diary-detail/:id?`, `/diary-search`, `/settings`。后退导航会自动销毁离开页面的缓存。
- **Pinia Store**:
  - `config.ts` — 通过 Tauri Store 插件持久化到 `settings.json`。`useTauriConfig()` 返回一个与 Rust 后端双向自动同步的 Vue ref。配置项包含主题、生物识别开关、加密后的 OSS 配置、置顶日记 ID 等。
  - `data.ts` — 日记列表 ID、摘要缓存、当前编辑状态的内存存储。
- **Tauri 绑定** (`src/bindings.ts`): 由 tauri-specta 从 Rust 命令签名自动生成，**请勿手动编辑**。仅 Windows 调试构建时自动重新导出。
- **API 包装** (`src/utils/api.ts`): 解包 tauri-specta 的 `Result<T, E>` 类型——错误时 throw，成功时返回数据。
- **编辑器** (`src/components/editor/`): 手动实现的富文本编辑器，通过自定义扩展节点支持图片、视频、音频、通用文件等附件类型的内联展示。

### 后端 (`src-tauri/src/`)

每个领域模块目录包含 `mod.rs`、类型定义、命令、错误和测试：

| 模块 | 职责 |
|---|---|
| `cryptos` | AES-256-GCM 加解密、AES-256-CTR 流式加解密、Argon2id 密钥派生。`Crypto` 是 `Arc<RwLock<Option<DerivedKey>>>` 的可克隆句柄。 |
| `diaries` | 加密日记清单的 CRUD。每篇日记在 OSS 存储为 `{id}/manifest.enc`。标题取自正文首行。含迁移系统（V1→V2 为附件添加 etag 字段）。 |
| `attachments` | 附件管理（添加、删除、旋转、切换加密状态）。包含自定义 `attachment://` URI Scheme 协议，用于在界面中直接内联展示解密后的媒体内容。 |
| `object` | 对 `s3` crate 的封装，提供对 OSS 的流式上传/下载/删除操作。未加密附件使用预签名 URL 直接访问。 |
| `caches` | 两层缓存：`DiaryMemoryCache`（内存 DashMap，按日记 ID 索引）和 `LocalFileCache`（磁盘缓存，用 MD5 记录 etag 便于缓存校验）。 |
| `tasks` | `TaskPool` 管理可取消的异步任务，向前端返回取消令牌。 |
| `stream` | `ByteStream` 类型别名及相关工具：CTR 流加密适配器、数据收集、文件转流。 |
| `state` | `AppState`——中心化管理状态，持有 Crypto、OssClient（通过 OnceLock 延迟初始化）、缓存层、任务池。 |
| `storages` | OSS 路径工具：`remote_manifest_key(id)` → `"{id}/manifest.enc"`，`remote_attachments_key(id, filename)` → `"{id}/{filename}"`。 |
| `utils` | 文件工具、基于时间戳的降序 ID 生成、用于通过 Tauri Channel 发送类型化事件的 MessageSender trait。 |

### 加密流程

1. 用户输入主密码 → Argon2id 从密码 + salt 派生 256 位 DEK。
2. DEK 存储在 `Crypto` 的 `Arc<RwLock<Option<DerivedKey>>>` 中，会话期间有效。
3. 日记文本：JSON manifest 用 AES-256-GCM 加密（nonce 前置在密文前）。
4. 附件：AES-256-CTR 流式加密（nonce 存储在 `AttachmentMeta` 中），支持通过计数器偏移实现 Range 请求解密。
5. 生物识别解锁：将 DEK 的 hex 字符串存储在平台密钥库中，通过指纹/面容验证后取出。

### 缓存策略

- `DiaryMemoryCache`: `DashMap<String, Arc<(DiaryManifest, String)>>`，按日记 ID 索引，值为 (manifest, 远程 etag)。每次读取优先检查。
- `LocalFileCache`: 磁盘缓存目录 `{app_cache_dir}/lfc/`。每个 key 对应 `{key}.data` + `{key}.md5`。MD5 存储的是远程 etag，用于与 OSS 比对决定是否重新下载。

### 关键设计

- **URI Scheme 协议** (`attachment_protocol.rs`): 前端请求 `http://attachment.localhost/{diary_id}/{filename}`，协议处理器从 OSS/缓存获取数据并流式解密后返回。支持 HTTP Range 请求（最大单次 1MB），适用于视频拖动播放。
- **etag 缓存校验**: 读取日记时先 HEAD 请求获取远程 etag，与内存缓存和本地文件缓存比对，命中则跳过下载。详见 `diaries/diary.rs:get_diary()`。
- **迁移系统**: `DiaryManifest` 含 `version` 字段，`MigrationRegistry` 按版本顺序链式注册迁移步骤（当前 V2）。
- **前端配置持久化**: `useTauriConfig()` 创建 Vue `customRef`，初始化时自动读取、修改时自动保存，通过 `onKeyChange` 响应跨窗口变更。

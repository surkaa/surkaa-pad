# SurKaa Pad

> 端到端加密的私人日记应用，数据由你掌控。

## 项目简介

SurKaa Pad 是一款基于 [Tauri 2](https://tauri.app/) 构建的跨平台（桌面 & Android）私人日记软件。所有日记内容与附件均在**本地完成加密**后再同步至阿里云 OSS，云端仅存储密文，即便是云服务提供商也无法读取你的任何隐私数据。

只需准备一个阿里云 OSS Bucket 的访问密钥（AK）与一个自定义主密码，即可享受丝滑的端到端加密日记体验。

## 核心特性

- 🔐 **端到端加密**：日记正文采用 AES-256-GCM 加密，媒体附件采用 AES-256-CTR 流式加密；主密码通过 Argon2id 算法派生数据加密密钥（DEK），零知识架构保障隐私安全。
- ☁️ **云端同步**：以阿里云 OSS 作为加密数据的存储后端，随时随地跨设备访问你的日记。
- 📝 **富文本日记**：支持创建、编辑、删除日记，提供流畅的文字书写体验。
- 🔍 **全文搜索**：对日记内容进行本地全文检索，快速找到历史记录。
- 🖼️ **媒体附件**：支持在日记中附加图片与视频，附件同样经过端到端加密存储与传输；支持直接调用相机拍照附加；支持图片旋转与附件单独加密/解密切换。
- 👆 **生物识别解锁**（Android）：支持指纹或面容快速解锁应用，免去每次输入主密码的繁琐。
- 🌗 **深色 / 浅色 / 跟随系统**：三种主题模式随心切换。
- 📱 **跨平台**：基于 Tauri 2 构建，同时支持 Windows 桌面端与 Android 移动端。
- 📦 **数据管理**：支持导出日志文件，支持一键重置应用配置。

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端框架 | [Vue 3](https://vuejs.org/) + TypeScript |
| UI 组件库 | [Quasar Framework](https://quasar.dev/) |
| 桌面/移动壳 | [Tauri 2](https://tauri.app/)（Rust） |
| 加密算法 | AES-256-GCM（文本） / AES-256-CTR（流媒体） / Argon2id（密钥派生） |
| 云存储 | 阿里云 OSS |
| 构建工具 | [Vite](https://vite.dev/) + [pnpm](https://pnpm.io/) |

## 开发与构建

### 环境准备

- [Node.js](https://nodejs.org/) >= 18
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install) 工具链（stable）
- [Tauri CLI 2](https://tauri.app/start/prerequisites/)

### 安装依赖

```bash
pnpm install
```

### 本地开发（桌面）

```bash
pnpm tauri:msi:dev
```

### 本地开发（Android）

```bash
pnpm tauri:android:dev
```

### 生产构建（桌面 MSI）

```bash
pnpm tauri:msi:build
```

### 生产构建（Android APK / AAB）

```bash
pnpm build:android:build
```

---

# 阿里云配置指南

## 配置所需信息

要使用 SurKaa Pad，需要准备以下阿里云 OSS 配置信息：

| 配置项                  | 描述                                           | 获取位置                           |
|----------------------|----------------------------------------------|--------------------------------|
| `ALIYUN_KEY`         | 阿里云 AccessKey ID，用于身份验证                      | 阿里云控制台 → 访问控制 → AccessKey 管理   |
| `ALIYUN_SECRET`      | 阿里云 AccessKey Secret，用于签名验证                  | 阿里云控制台 → 访问控制 → AccessKey 管理   |
| `ALIYUN_BUCKET_NAME` | OSS 存储桶名称，用于存储加密的日记数据                        | 阿里云 OSS 控制台 → 存储桶列表 → 创建的存储桶名称 |
| `ALIYUN_ENDPOINT`    | OSS 访问域名，格式如：`oss-cn-guangzhou.aliyuncs.com` | 阿里云 OSS 控制台 → 存储桶概览 → 访问域名     |

## 获取步骤

### 1. 注册阿里云账号
- 访问 [阿里云官网](https://aliyun.com/minisite/goods?userCode=1pxgzrjg)
- 点击注册并完成账号认证

### 2. 开通 OSS 包年服务（按量收费则不需求此步骤）
- 登录阿里云控制台(或直接访问[阿里云对象存储](https://www.aliyun.com/product/oss))
- 搜索并进入"对象存储 OSS"服务
- 首次使用需开通服务（通常有免费额度）

### 3. 创建存储桶
- 在 OSS 控制台点击"创建 Bucket"
- 填写存储桶名称
- 选择地域（建议选择距离自己近的地区）
- 其他设置可使用默认值
- 创建成功后记录下：
  - **存储桶名称** → `ALIYUN_BUCKET_NAME`
  - **Endpoint** → `ALIYUN_ENDPOINT`（格式如：`oss-cn-guangzhou.aliyuncs.com`）

### 4. 创建访问密钥
- 在控制台右上角悬停头像，进入"AccessKey 管理"
- 创建新的 AccessKey（如有安全提示请按需设置）
- 创建成功后保存：
  - **AccessKey ID** → `ALIYUN_KEY`
  - **AccessKey Secret** → `ALIYUN_SECRET`

### 5. 设置访问权限
- 回到 OSS 控制台，选择您创建的存储桶
- 进入"权限管理" → "Bucket 授权策略"
- 添加允许访问刚刚创建的存储桶的权限策略

## 配置完成

获取到四个配置信息后，在 SurKaa Pad 的解锁界面填入相应字段即可开始使用端到端加密的日记同步功能。

**注意**：所有日记数据 (包括媒体文件) 都会在本地加密后再上传到 OSS，阿里云无法解密您的日记内容。

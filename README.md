# SurKaa Pad

端到端加密日记软件  

需要有阿里云OSS的Bucket的AK密钥，同时和一个主密码即可享受端到端加密的日记体验。

## TODO

- 添加立即拍摄并上传的即时功能

# 阿里云配置指南

## 配置所需信息

要使用 SurKaa Pad，需要准备以下阿里云 OSS 配置信息：

| 配置项 | 描述 | 获取位置 |
|--------|------|----------|
| `ALIYUN_KEY` | 阿里云 AccessKey ID，用于身份验证 | 阿里云控制台 → 访问控制 → AccessKey 管理 |
| `ALIYUN_SECRET` | 阿里云 AccessKey Secret，用于签名验证 | 阿里云控制台 → 访问控制 → AccessKey 管理 |
| `ALIYUN_BUCKET_NAME` | OSS 存储桶名称，用于存储加密的日记数据 | 阿里云 OSS 控制台 → 存储桶列表 → 创建的存储桶名称 |
| `ALIYUN_ENDPOINT` | OSS 访问域名，格式如：`oss-cn-hangzhou.aliyuncs.com` | 阿里云 OSS 控制台 → 存储桶概览 → 访问域名 |

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
  - **Endpoint** → `ALIYUN_ENDPOINT`（格式如：`oss-cn-hangzhou.aliyuncs.com`）

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

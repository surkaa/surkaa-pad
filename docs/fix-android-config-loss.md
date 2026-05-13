# Fix: Android 配置文件丢失问题

## 问题描述

Android 上偶发配置文件（`settings.json`）丢失，表现为：
- 生物识别解锁不弹起
- 退出应用后再次进入显示首次配置界面
- 但缓存的日记和附件仍在（存储占用正常）

Windows 上从未复现。

## 根因分析

问题由 `tauri-plugin-store` v2.4.2 的文件写入机制 + Android 进程管理共同导致。

### 故障链路

```
[正常运行] settings.json 内容完好，内存 store.cache 有数据
     │
     ▼
[用户切后台] Android 可能随时杀进程
     │
     ├─ 如果此时 auto_save（默认 100ms 防抖）正在执行 fs::write
     └─ 或正在进行的 save() 调用
     │
     ▼
[文件损坏] fs::write 是非原子写入，进程被杀时文件被截断为空或半写
     │
     ▼
[下次启动] Store.load() → store_inner.load() 读文件
     │
     ▼
[静默失败] 反序列化失败 → 错误被 `let _ =` 丢弃 → store.cache 只有空 defaults
     │         （见 tauri-plugin-store store.rs:222）
     ▼
[前端读取] getNormalConfig('encrypted_oss_config') → null
           getNormalConfig('biometric_enabled') → false（默认值）
     │
     ▼
[表现] 不弹生物识别，显示首次配置界面
     │
     ▼
[数据覆写] auto_save 或显式 set() 把空 cache 写入磁盘 → 配置永久丢失
```

### 关键代码位置

**插件侧（tauri-plugin-store v2.4.2）：**

| 问题 | 文件 | 行号 | 说明 |
|------|------|------|------|
| 非原子写入 | `store.rs` | 296 | `fs::write(&self.path, bytes)` — 进程被杀时截断 |
| 加载错误静默丢弃 | `store.rs` | 222 | `let _ = store_inner.load()` — 损坏时不报错 |
| 默认 auto_save | `store.rs` | 68 | `auto_save: Some(Duration::from_millis(100))` — 100ms 后自动写磁盘 |
| Exit 事件不可靠 | `lib.rs` | 448-460 | `RunEvent::Exit` 在 Android 强杀时不触发 |

**项目侧：**

| 文件 | 说明 |
|------|------|
| `src/stores/config.ts` | Store 初始化、读写，无备份恢复机制 |
| `src/views/unlock/Unlock.vue` | onMounted 读配置决定显示登录还是首次配置 |
| `src-tauri/src/lib.rs:115` | 插件注册使用全部默认配置 |

### 为什么缓存数据不受影响

日记和附件缓存在 `{app_cache_dir}/lfc/` 目录，配置文件在 `{app_data_dir}/settings.json`。两者是独立的文件，配置损坏不影响缓存。

## 修复方案

在 `src/stores/config.ts` 中加入**备份/恢复机制**：

1. **`ensureStoreIntegrity()`** — 在 `Store.load()` 之前检查 `settings.json` 完整性：
   - 文件不存在 → 检查 `settings.json.bak`，有则恢复
   - 文件为空（被截断）→ 从备份恢复
   - JSON 解析失败（损坏）→ 从备份恢复

2. **`initStore()`** — 加载成功后，如果 store 有数据，创建 `settings.json.bak` 备份

3. **`saveNormalConfig()`** — 每次成功保存后同步更新备份

4. **`deleteConfig()`** — 删除操作后也同步备份（避免重置配置后备份还是旧数据）

### 为什么不能在插件侧修复

- `tauri-plugin-store` 是第三方 crate，`fs::write` 是其内部实现
- 理想方案是用原子写入（write-to-temp + rename），但需要 fork 插件
- 应用层备份是成本最低的有效方案

## 当前状态

- [x] 根因分析完成
- [x] 代码修改完成（`src/stores/config.ts`）
- [ ] TypeScript 类型检查（本地无开发环境，待验证）
- [ ] Android 真机测试
- [ ] 首次安装场景验证（无备份文件时应正常工作）

## 测试要点

1. **正常流程**：首次安装 → 配置 → 退出 → 重新进入，配置应保留
2. **备份创建**：配置保存后，`{app_data_dir}/` 下应出现 `settings.json.bak`
3. **恢复流程**：手动删除/清空 `settings.json`，重新打开应用应从备份恢复
4. **重置配置**：设置中重置后，备份也应被清空（不会恢复旧配置）
5. **首次安装**：无任何文件时，应正常进入首次配置（不应报错）

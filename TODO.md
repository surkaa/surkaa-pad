# TODO

项目级待办与架构债务集中记录在这里。完成后删除对应条目，并在提交信息中说明。

## P1：保存与存储一致性

- [ ] 日记自动保存可能并发执行应该同一时间只能有一个保存请求，窗口关闭、页面离开或 KeepAlive 停用时也可能不会完整等待待保存内容和正在执行的保存。

- [ ] `storage_mode_gate` 没有覆盖所有存储读写操作，部分命令、后台任务和附件 HTTP 请求可能与存储模式切换并发。

## P1：平台隔离与最小权限

- [ ] Tauri capability 包含范围过大的文件系统权限。

## P2：前端与依赖清理

- [ ] 已不再需要的 `tauri-plugin-store` 旧配置迁移代码、依赖和 capability 仍然存在。

- [ ] `DiaryDetail`、`DiarySearch` 和 `Settings` 为同步导入，非首屏代码进入了启动主包。

- [ ] 编辑器占位/选中态#adb5bd、工具栏#fff、搜索筛选white和图片预览仍有未适配浅色、深色主题的硬编码颜色。

## P3：代码组织

- [ ] `src-tauri/src/local_storage/migration.rs` 混合了不同版本的迁移逻辑、公共流程和大量测试，模块职责过重，考虑按不同版本拆分。

## AI Agent

- [ ] 仅在 Windows 端接入本地部署的 AI Agent，确保日记数据不发送至外部平台，Android 端暂不支持。

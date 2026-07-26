# TODO

项目级待办与架构债务集中记录在这里。完成后删除对应条目，并在提交信息中说明。

## 高优先级

- [ ] 修复并发附件元数据偶发丢失
  - `test_thread_add_and_delete_attachment` 在全量并行测试中偶发只保存 9/10 条元数据，单独运行可以通过。
  - 查清测试间干扰、日记 ID 冲突及 manifest read-modify-write 锁的作用域。

## 存储与附件

- [ ] 明确 `LocalFileCache` 的职责和命名
  - 本地模式下它是持久化对象存储，远程模式下又承担写透缓存，当前名称无法表达双重职责。
  - 评估重命名为 `LocalObjectStore`，或拆分持久化存储与缓存接口。

- [ ] 收敛分片上传边界
  - multipart 方法仍暴露在 `DiaryStore` trait 中，但实际流程主要由 `attachment_command` 和 `ChunkedUploadState` 驱动。
  - 移除未使用接口，或让分片上传完整地通过 Store 抽象执行，消除当前 Clippy 警告。

- [ ] 加强附件同步有效性校验
  - 当前同步会忽略没有 `manifest.enc` 的孤立目录。
  - 后续应解析源端 manifest，只同步 `attachments` 元数据中存在的附件，避免有效日记目录中的未引用文件被迁移。

- [ ] 将本地附件 HTTP 响应改为真正流式输出
  - 当前服务支持 Range，但会先把单次响应收集为 `Full<Bytes>`；大图片或大范围请求仍可能产生较高内存峰值。

## 前端与领域模型

- [ ] 继续拆分 `DiaryDetail.vue`
  - 将附件重命名、未使用附件清理、图集操作和编辑器事件编排下沉到独立 composable 或领域工具。

- [ ] 降低正文节点与附件元数据的双写风险
  - `DiaryContent.nodes` 和 `DiaryManifest.attachments` 通过文件名关联，重命名、删除和并发上传必须同时维护两份状态。
  - 评估使用稳定附件 ID 关联，或由后端提供原子领域操作。

- [ ] 完善上传进度弹窗
  - 支持取消、明确展示失败项，并在上传期间阻止直接退出页面造成任务状态不明。

## 配置与构建

- [ ] 收敛云存储状态来源
  - 前端持久化 `remote_enabled`，Rust `AppState` 保存运行时状态；继续减少两者短暂不一致的可能性，并覆盖异常恢复测试。

- [ ] 提升本地 Fork 依赖的可复现性
  - `rust-s3`、`infer`、生物识别插件等依赖工作区外的 `../Forks` 路径。
  - 评估改为固定 Git revision、workspace 子模块或发布版本，使干净环境和 CI 可以直接构建。

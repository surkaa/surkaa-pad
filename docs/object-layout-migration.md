# 对象布局一次性迁移

本说明只用于将旧日记对象布局一次性迁移到当前固定布局，不是通用的存储版本迁移框架。

旧布局：

```text
{diary_id}/manifest.enc
{diary_id}/{attachment_id}
```

当前布局：

```text
diaries/{diary_id}/manifest.enc
diaries/{diary_id}/attachments/{attachment_id}
```

迁移使用同一 Bucket 内的 OSS 服务端复制。复制阶段保留所有旧对象；只有新对象通过大小和 ETag 校验后，清理命令才允许删除对应旧对象。`ai/`、`rust-tests/` 等其他命名空间不会参与迁移。

## 八步操作

1. 确认日记内容均已升级为当前数据版本，并关闭所有仍可能写入该 Bucket 的旧版应用。
2. 为正式 Bucket 开启版本控制或创建可恢复的备份，然后在 `src-tauri/.env` 中填写正式 Bucket 的四项配置。不要提交 `.env`。
3. 在 `src-tauri/` 下执行只读检查：

   ```powershell
   cargo run --bin oss_tool -- layout-plan
   ```

4. 审核结果：`目标冲突` 和 `旧目录异常对象` 必须都是 `0`；记录旧结构对象数量和总大小。需要逐项查看时再追加 `--details`。
5. 只复制并校验新对象，其中 `<bucket>` 必须与 `.env` 中的 Bucket 名完全一致：

   ```powershell
   cargo run --bin oss_tool -- layout-copy --confirm-bucket <bucket>
   ```

6. 再次运行 `layout-plan`。此时 `待复制` 应为 `0`，全部旧对象应归入 `已复制且一致`。然后用新代码启动应用，核对日记数量，并抽查包含图片、音频、视频和大附件的日记。复制验证期间不要重新启用旧版应用写入。
7. 确认新版本读写正常后，删除已验证的旧对象：

   ```powershell
   cargo run --bin oss_tool -- layout-cleanup --confirm-bucket <bucket>
   ```

8. 最后再次运行 `layout-plan`：`旧结构对象`、`待复制`、`已复制且一致` 和冲突项都应为 `0`，新结构对象数量应保持不变。各设备第一次启动新版本时，会在自己的 LOS 内通过同盘重命名完成相同布局调整。

## 安全约束

- `layout-copy` 不覆盖执行期间突然出现的目标对象；发现并发写入或内容变化会立即停止。
- `layout-cleanup` 在存在待复制对象、目标冲突或异常旧目录对象时拒绝运行。
- 形如 `123456789/` 的零字节 OSS 目录标记不包含日记数据，迁移时会明确列出并忽略，不复制也不自动删除；非零或更深层的异常对象仍会阻止迁移。
- 清理前会再次核对每一对源/目标对象，附件先删除，旧 manifest 最后删除。
- 命令中断后可以原样重跑；复制和清理操作均按当前 Bucket 状态重新生成计划。

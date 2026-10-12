import type {KeyChangeFileStep, KeyChangeProgress} from '../bindings';

// 将当前对象的各步骤进度折算成原数据字节；这不是网络传输字节或校验完成量。
// 所有步骤同等加权只是估算，云内复制、请求等待等耗时会自然计入实测平均速度。
const STEP_INDEX: Record<KeyChangeFileStep, number> = {
  writingStage: 0, checkingStageContent: 1, checkingTarget: 2, checkingStage: 3,
  backingUp: 4, checkingBackup: 5, writingTarget: 6, checkingCommitted: 7,
};
const STEP_COUNT = 8;
const MIN_SAMPLE_MS = 5000;
const MAX_SAMPLE_BYTES = 1024 * 1024;

/** 仅在当前运行期间测速；续改 reset 后不把已完成字节或暂停时间计入速度。 */
export class KeyChangeTimeEstimator {
  private startedAt = 0;
  private baselineBytes: number | null = null;
  private completedBytes = 0;
  private processedBytes = 0;
  private totalBytes = 0;
  private currentFile: {key: string; checkpointBytes: number; size: number} | null = null;

  reset(now: number) {
    this.startedAt = now;
    this.baselineBytes = null;
    this.completedBytes = this.processedBytes = this.totalBytes = 0;
    this.currentFile = null;
  }

  update(progress: KeyChangeProgress) {
    if (!Number.isFinite(progress.totalBytes) || progress.totalBytes < 0
      || !Number.isFinite(progress.completedBytes) || progress.completedBytes < 0) return;
    const completed = Math.min(progress.completedBytes, progress.totalBytes);
    if (completed < this.completedBytes) return; // 不让延迟到达的旧事件倒退速度样本。
    this.totalBytes = progress.totalBytes;
    this.baselineBytes ??= completed;
    this.completedBytes = completed;
    let processed = completed;
    const file = progress.fileProgress;
    if (file && progress.currentFile && Number.isFinite(file.totalBytes) && file.totalBytes > 0
      && Number.isFinite(file.totalPoints) && file.totalPoints > 0
      && Number.isFinite(file.completedPoints) && STEP_INDEX[file.step] !== undefined) {
      if (this.currentFile?.key !== progress.currentFile || this.currentFile.checkpointBytes !== completed) {
        const size = Math.min(file.totalBytes, this.totalBytes - completed);
        // 续改可能从已验证的临时密文开始；此前已做完的步骤不能冒充本次处理量。
        if (!this.currentFile && completed === this.baselineBytes) {
          this.baselineBytes += size * STEP_INDEX[file.step] / STEP_COUNT;
        }
        this.currentFile = {key: progress.currentFile, checkpointBytes: completed, size};
      }
      const fraction = Math.max(0, Math.min(file.completedPoints / file.totalPoints, 1));
      processed += this.currentFile.size * (STEP_INDEX[file.step] + fraction) / STEP_COUNT;
    }
    // 步骤切换会临时清空 fileProgress；已观察到的处理量不因此倒退或重复累计。
    this.processedBytes = Math.min(this.totalBytes, Math.max(this.processedBytes, processed));
  }

  remainingMs(now: number): number | null {
    if (this.baselineBytes === null || this.totalBytes <= 0 || !Number.isFinite(now)) return null;
    if (this.completedBytes >= this.totalBytes) return 0;
    const elapsed = now - this.startedAt;
    const processed = this.processedBytes - this.baselineBytes;
    // 避免仅凭几个很小的元数据对象预测整个大 Vault；小任务按剩余数据的 1% 采样。
    const minBytes = Math.min(MAX_SAMPLE_BYTES, (this.totalBytes - this.baselineBytes) * 0.01);
    if (elapsed < MIN_SAMPLE_MS || processed <= 0 || processed < minBytes) return null;
    const remaining = (this.totalBytes - this.processedBytes) * (elapsed / processed);
    return Number.isFinite(remaining) && remaining >= 0 ? remaining : null;
  }
}

/** 本地预计完成时刻；跨日带日期，避免把明天的时间误当成今天。 */
export function keyChangeEstimatedCompletion(remainingMs: number, now: Date): string | null {
  if (!Number.isFinite(remainingMs) || remainingMs < 0 || !Number.isFinite(now.getTime())) return null;
  const end = new Date(now.getTime() + remainingMs);
  if (!Number.isFinite(end.getTime())) return null;
  const sameDay = now.toDateString() === end.toDateString();
  return new Intl.DateTimeFormat('zh-CN', {
    ...(!sameDay ? {month: 'numeric', day: 'numeric'} as const : {}),
    ...(now.getFullYear() !== end.getFullYear() ? {year: 'numeric'} as const : {}),
    hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
  }).format(end);
}

import type {KeyChangeReport} from '../bindings';

export const KEY_CHANGE_REPORT_NOTES = [
  '仅统计数据迁移及后端最终确认；准备阶段单独记录。后端活跃用时不含暂停、密码输入和前端保存配置的等待。',
  '读取字节是实际消费的对象内容；写入字节是已交给写入端的内容，失败时不代表全部提交。它们不是含协议开销或 SDK 内部重试的线上流量。',
  'readOpen 是取得读取响应流的时间（旧实现还可能包含额外元信息查询）；readAwait 是等待下一块数据的累计时间，含磁盘/网络与任务调度，不含消费者处理间隔。',
  '加解密和 SHA-256 仅统计同步数据处理；不含网络等待、派生密钥和准备阶段。它们是墙钟耗时，不是操作系统 CPU 使用率。',
  'operations 是逻辑存储调用数，不是 HTTP 请求数；读取生命周期、上传、复制及其内部操作可能重叠，不能将这些耗时相加当作总用时。',
  '云内复制的字节单独计数，不计入本机上传/下载；报告不包括 HTTP 头、云锁/bootstrap 等少量控制请求。',
  '最多保留十个最慢的匿名对象处理尝试，同一对象的重试可能重复出现。强制退出只能保留最近检查点的统计，incompleteHistory 为 true 时不是完整历史。',
  'application 是当前查看报告的应用版本；升级应用后可能不等于改密开始时的版本。',
];

export function keyChangeReportSource(report: KeyChangeReport, context: {appVersion: string; gitCommit: string}) {
  return {
    reportType: 'vault-key-change-performance',
    application: {version: context.appVersion, gitCommit: context.gitCommit},
    notes: KEY_CHANGE_REPORT_NOTES,
    measurements: report,
  };
}

export function keyChangeReportOutcome(outcome: string): string {
  const labels: Record<string, string> = {
    running: '正在改密', paused: '已暂停', failed: '处理失败，可续改',
    awaitingConfirmation: '数据已处理，等待本机配置确认',
    confirmationFailed: '本机确认未完成，可续改', completed: '改密已完成',
  };
  return labels[outcome] ?? '改密统计';
}

export function reportDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '—';
  if (ms < 1000) return `${Math.round(ms)} 毫秒`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)} 秒`;
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
}

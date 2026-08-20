export interface SyncProgressDisplay {
  progress: number
  total: number
  statusText: string
  currentFile: string
  fileDetail: string
}

interface RuntimeSyncEvent {
  event: string
  data?: Record<string, unknown>
}

export function initialSyncProgressDisplay(statusText = ''): SyncProgressDisplay {
  return {
    progress: 0,
    total: 0,
    statusText,
    currentFile: '',
    fileDetail: '',
  }
}

export function reduceSyncProgressDisplay(
  current: SyncProgressDisplay,
  msg: RuntimeSyncEvent,
): SyncProgressDisplay {
  const data = msg.data || {}
  const action = data.direction === 'download' ? '下载' : '上传'
  if (msg.event === 'preparing') {
    return initialSyncProgressDisplay('正在检查本地与云端文件...')
  }
  if (msg.event === 'started') {
    const totalFiles = numberValue(data.totalFiles)
    const skippedFiles = numberValue(data.skippedFiles)
    return {
      ...current,
      progress: 0,
      total: numberValue(data.totalBytes),
      statusText: totalFiles > 0
        ? `准备${action} ${totalFiles} 个文件${skippedFiles > 0 ? `，已跳过 ${skippedFiles} 个` : ''}`
        : `文件均为最新${skippedFiles > 0 ? `，已跳过 ${skippedFiles} 个` : ''}`,
      currentFile: '',
      fileDetail: '',
    }
  }
  if (msg.event === 'progress') {
    const phase = syncPhaseText(stringValue(data.phase))
    return {
      progress: numberValue(data.transferredBytes),
      total: numberValue(data.totalBytes),
      statusText: `正在${action}${phase} ${numberValue(data.currentFileIndex)}/${numberValue(data.totalFiles)}`,
      currentFile: stringValue(data.currentFile),
      fileDetail: `${formatBytes(numberValue(data.currentFileBytes))} / ${formatBytes(numberValue(data.currentFileSize))}`,
    }
  }
  if (msg.event === 'completed') {
    return {
      progress: current.total,
      total: current.total,
      statusText: `${action}完成，共传输 ${numberValue(data.transferredFiles)} 个文件，跳过 ${numberValue(data.skippedFiles)} 个`,
      currentFile: '',
      fileDetail: formatBytes(numberValue(data.transferredBytes)),
    }
  }
  if (msg.event === 'error') {
    return {
      ...current,
      statusText: `同步失败：${stringValue(data.message) || '未知错误'}`,
      currentFile: stringValue(data.currentFile),
      fileDetail: '',
    }
  }
  return current
}

function syncPhaseText(phase: string): string {
  switch (phase) {
    case 'attachments':
      return '附件'
    case 'aiMessages':
      return 'AI 消息'
    case 'manifests':
      return '日记主文件'
    case 'aiSessions':
      return 'AI 会话信息'
    default:
      return '数据'
  }
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / (1024 ** unit)
  return `${value >= 100 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`
}

function numberValue(value: unknown): number {
  const number = Number(value)
  return Number.isFinite(number) && number >= 0 ? number : 0
}

function stringValue(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

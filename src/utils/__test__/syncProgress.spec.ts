import { describe, expect, it } from 'vitest'
import {
  formatBytes,
  initialSyncProgressDisplay,
  reduceSyncProgressDisplay,
} from '../syncProgress'

describe('reduceSyncProgressDisplay', () => {
  it('uses an indeterminate preparing state before the plan is ready', () => {
    const result = reduceSyncProgressDisplay(
      initialSyncProgressDisplay('旧状态'),
      { event: 'preparing', data: { direction: 'upload' } },
    )

    expect(result).toEqual(initialSyncProgressDisplay('正在检查本地与云端文件...'))
  })

  it('shows planned bytes, files and skipped files', () => {
    const result = reduceSyncProgressDisplay(initialSyncProgressDisplay(), {
      event: 'started',
      data: { direction: 'upload', totalFiles: 3, totalBytes: 4096, skippedFiles: 2 },
    })

    expect(result.total).toBe(4096)
    expect(result.statusText).toBe('准备上传 3 个文件，已跳过 2 个')
  })

  it('shows global and current attachment progress', () => {
    const result = reduceSyncProgressDisplay(initialSyncProgressDisplay(), {
      event: 'progress',
      data: {
        direction: 'download',
        phase: 'attachments',
        currentFile: 'diary/photo.jpg',
        currentFileIndex: 2,
        totalFiles: 5,
        currentFileBytes: 1024,
        currentFileSize: 2048,
        transferredBytes: 3072,
        totalBytes: 8192,
      },
    })

    expect(result).toMatchObject({
      progress: 3072,
      total: 8192,
      statusText: '正在下载附件 2/5',
      currentFile: 'diary/photo.jpg',
      fileDetail: '1.0 KB / 2.0 KB',
    })
  })

  it('keeps the known total when completing and reports failures safely', () => {
    const current = { ...initialSyncProgressDisplay(), total: 100, progress: 75 }
    const completed = reduceSyncProgressDisplay(current, {
      event: 'completed',
      data: { direction: 'upload', transferredFiles: 2, skippedFiles: 4, transferredBytes: 100 },
    })
    expect(completed.progress).toBe(100)
    expect(completed.statusText).toBe('上传完成，共传输 2 个文件，跳过 4 个')

    const failed = reduceSyncProgressDisplay(current, {
      event: 'error',
      data: { direction: 'download', currentFile: 'broken.bin', message: '网络错误' },
    })
    expect(failed.statusText).toBe('同步失败：网络错误')
    expect(failed.currentFile).toBe('broken.bin')
  })
})

describe('formatBytes', () => {
  it('formats zero and large byte values', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
  })
})

import type {
  LocalStorageMigrationEvent,
  LocalStorageMigrationPhase,
} from '../bindings';
import {formatBytes} from './format';

export interface LocalStorageMigrationDisplay {
  sourcePath: string;
  targetPath: string;
  phase: LocalStorageMigrationPhase;
  statusText: string;
  currentFile: string;
  fileDetail: string;
  progress: number;
  total: number;
  completed: boolean;
  error: string;
  cleanupWarning: string;
}

const PHASE_TEXT: Record<LocalStorageMigrationPhase, string> = {
  preparing: '正在准备本地数据迁移…',
  copying: '正在复制本地数据…',
  verifying: '正在校验迁移结果…',
  switching: '正在切换本地存储位置…',
  cleaning: '正在清理旧位置…',
};

export function initialLocalStorageMigrationDisplay(): LocalStorageMigrationDisplay {
  return {
    sourcePath: '',
    targetPath: '',
    phase: 'preparing',
    statusText: PHASE_TEXT.preparing,
    currentFile: '',
    fileDetail: '',
    progress: 0,
    total: 0,
    completed: false,
    error: '',
    cleanupWarning: '',
  };
}

export function withLocalStorageMigrationError(
  state: LocalStorageMigrationDisplay,
  error: string,
): LocalStorageMigrationDisplay {
  return {
    ...state,
    statusText: '本地数据迁移失败',
    error,
  };
}

export function reduceLocalStorageMigrationDisplay(
  state: LocalStorageMigrationDisplay,
  message: LocalStorageMigrationEvent,
): LocalStorageMigrationDisplay {
  switch (message.event) {
    case 'preparing':
      return {
        ...state,
        sourcePath: message.data.sourcePath,
        targetPath: message.data.targetPath,
        phase: 'preparing',
        statusText: PHASE_TEXT.preparing,
      };
    case 'started':
      return {
        ...state,
        total: message.data.totalBytes,
        statusText: message.data.fastMove
          ? '正在移动本地数据…'
          : PHASE_TEXT.copying,
      };
    case 'phase':
      return {
        ...state,
        phase: message.data.phase,
        statusText: PHASE_TEXT[message.data.phase],
        currentFile: '',
        fileDetail: '',
      };
    case 'progress': {
      const {data} = message;
      return {
        ...state,
        phase: data.phase,
        statusText: PHASE_TEXT[data.phase],
        currentFile: data.currentFile,
        fileDetail: `${data.currentFileIndex}/${data.totalFiles} · ${formatBytes(data.currentFileBytes)} / ${formatBytes(data.currentFileSize)}`,
        progress: data.processedBytes,
        total: data.totalBytes,
      };
    }
    case 'completed':
      return {
        ...state,
        targetPath: message.data.targetPath,
        statusText: '本地数据迁移完成',
        currentFile: '',
        fileDetail: `${message.data.migratedFiles} 个文件 · ${formatBytes(message.data.migratedBytes)}`,
        progress: message.data.migratedBytes,
        total: message.data.migratedBytes,
        completed: true,
        error: '',
        cleanupWarning: message.data.cleanupWarning ?? '',
      };
    case 'error':
      return withLocalStorageMigrationError(state, message.data.message);
  }
}

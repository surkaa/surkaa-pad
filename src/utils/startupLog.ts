import {error as writeErrorLog, info as writeInfoLog} from '@tauri-apps/plugin-log';

function elapsedMillis(): number {
  return Math.round(performance.now());
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}`;
  }
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export function logStartupPhase(phase: string): void {
  const message = `[startup +${elapsedMillis()}ms] ${phase}`;
  console.info(message);
  void writeInfoLog(message).catch(error => {
    console.warn('写入启动日志失败:', error);
  });
}

export function logStartupError(phase: string, error: unknown): void {
  const message = `[startup +${elapsedMillis()}ms] ${phase}: ${errorMessage(error)}`;
  console.error(message, error);
  void writeErrorLog(message).catch(logError => {
    console.warn('写入启动错误日志失败:', logError);
  });
}

export function installStartupErrorHandlers(): void {
  window.addEventListener('error', event => {
    logStartupError('window error', event.error ?? event.message);
  });
  window.addEventListener('unhandledrejection', event => {
    logStartupError('unhandled promise rejection', event.reason);
  });
}

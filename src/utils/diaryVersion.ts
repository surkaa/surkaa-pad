import type {
  DiaryVersionEvent,
  DiaryVersionItemOutcome,
  DiaryVersionOperation,
  DiaryVersionReport,
  DiaryVersionStorageScope,
} from '../bindings';

export type DiaryVersionDisplayPhase = 'idle' | 'running' | 'completed' | 'cancelled' | 'failed';

export interface DiaryVersionDisplayState {
  phase: DiaryVersionDisplayPhase;
  operation: DiaryVersionOperation | null;
  scope: DiaryVersionStorageScope | null;
  processed: number;
  total: number;
  currentDiaryId: string;
  currentOutcome: DiaryVersionItemOutcome | null;
  report: DiaryVersionReport | null;
  error: string;
}

export function initialDiaryVersionDisplay(
  operation: DiaryVersionOperation | null = null,
): DiaryVersionDisplayState {
  return {
    phase: operation ? 'running' : 'idle',
    operation,
    scope: null,
    processed: 0,
    total: 0,
    currentDiaryId: '',
    currentOutcome: null,
    report: null,
    error: '',
  };
}

export function withDiaryVersionError(
  state: DiaryVersionDisplayState,
  error: string,
): DiaryVersionDisplayState {
  return {
    ...state,
    phase: 'failed',
    currentDiaryId: '',
    currentOutcome: null,
    error,
  };
}

export function reduceDiaryVersionEvent(
  state: DiaryVersionDisplayState,
  message: DiaryVersionEvent,
): DiaryVersionDisplayState {
  switch (message.event) {
    case 'started':
      return {
        ...initialDiaryVersionDisplay(message.data.operation),
        scope: message.data.scope,
        total: message.data.total,
      };
    case 'progress':
      return {
        ...state,
        phase: 'running',
        operation: message.data.operation,
        processed: message.data.processed,
        total: message.data.total,
        currentDiaryId: message.data.diaryId,
        currentOutcome: message.data.outcome,
      };
    case 'completed':
      return terminalState('completed', message.data.operation, message.data.report);
    case 'cancelled':
      return terminalState('cancelled', message.data.operation, message.data.report);
    case 'error':
      return withDiaryVersionError(
        {...state, operation: message.data.operation},
        message.data.message,
      );
  }
}

function terminalState(
  phase: 'completed' | 'cancelled',
  operation: DiaryVersionOperation,
  report: DiaryVersionReport,
): DiaryVersionDisplayState {
  return {
    phase,
    operation,
    scope: report.scope,
    processed: report.processedDiaries,
    total: report.totalDiaries,
    currentDiaryId: '',
    currentOutcome: null,
    report,
    error: '',
  };
}

export function isDiaryVersionReportCurrent(report: DiaryVersionReport): boolean {
  return report.processedDiaries === report.totalDiaries
    && report.currentDiaries === report.totalDiaries
    && report.legacyDiaries === 0
    && report.newerDiaries === 0
    && report.failedDiaries === 0;
}

export function diaryVersionOutcomeText(outcome: DiaryVersionItemOutcome | null): string {
  switch (outcome) {
    case 'current':
      return '当前版本';
    case 'legacy':
      return '旧版';
    case 'newer':
      return '高于当前版本';
    case 'upgraded':
      return '已升级';
    case 'failed':
      return '读取或升级失败';
    default:
      return '';
  }
}

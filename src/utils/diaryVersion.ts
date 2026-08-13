import type {
  DiaryVersionEvent,
  DiaryVersionItemOutcome,
  DiaryVersionReport,
  DiaryVersionStorageScope,
} from '../bindings';

export type DiaryVersionDisplayPhase = 'idle' | 'running' | 'completed' | 'cancelled' | 'failed';

export interface DiaryVersionDisplayState {
  phase: DiaryVersionDisplayPhase;
  scope: DiaryVersionStorageScope | null;
  processed: number;
  total: number;
  currentDiaryId: number;
  currentOutcome: DiaryVersionItemOutcome | null;
  report: DiaryVersionReport | null;
  error: string;
}

export function initialDiaryVersionDisplay(running = false): DiaryVersionDisplayState {
  return {
    phase: running ? 'running' : 'idle',
    scope: null,
    processed: 0,
    total: 0,
    currentDiaryId: 0,
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
    currentDiaryId: 0,
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
        ...initialDiaryVersionDisplay(true),
        scope: message.data.scope,
        total: message.data.total,
      };
    case 'progress':
      return {
        ...state,
        phase: 'running',
        processed: message.data.processed,
        total: message.data.total,
        currentDiaryId: message.data.diaryId,
        currentOutcome: message.data.outcome,
      };
    case 'completed':
      return terminalState('completed', message.data.report);
    case 'cancelled':
      return terminalState('cancelled', message.data.report);
    case 'error':
      return withDiaryVersionError(state, message.data.message);
  }
}

function terminalState(
  phase: 'completed' | 'cancelled',
  report: DiaryVersionReport,
): DiaryVersionDisplayState {
  return {
    phase,
    scope: report.scope,
    processed: report.processedDiaries,
    total: report.totalDiaries,
    currentDiaryId: 0,
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
    case 'failed':
      return '读取失败';
    default:
      return '';
  }
}

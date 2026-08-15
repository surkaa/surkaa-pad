import {describe, expect, it} from 'vitest';
import {
  initialLocalStorageMigrationDisplay,
  reduceLocalStorageMigrationDisplay,
  withLocalStorageMigrationError,
} from '../localStorageMigration';

describe('localStorageMigration display', () => {
  it('tracks source, target and copy progress', () => {
    let state = initialLocalStorageMigrationDisplay();
    state = reduceLocalStorageMigrationDisplay(state, {
      event: 'preparing',
      data: {sourcePath: 'C:/old/los', targetPath: 'D:/Diary/los'},
    });
    state = reduceLocalStorageMigrationDisplay(state, {
      event: 'progress',
      data: {
        phase: 'copying',
        currentFile: '123/att-1',
        currentFileIndex: 2,
        totalFiles: 4,
        currentFileBytes: 512,
        currentFileSize: 1024,
        processedBytes: 1536,
        totalBytes: 4096,
      },
    });

    expect(state.sourcePath).toBe('C:/old/los');
    expect(state.targetPath).toBe('D:/Diary/los');
    expect(state.statusText).toBe('正在复制本地数据…');
    expect(state.currentFile).toBe('123/att-1');
    expect(state.fileDetail).toBe('2/4 · 512 B / 1 KB');
    expect(state.progress).toBe(1536);
  });

  it('shows verification and completion details', () => {
    let state = initialLocalStorageMigrationDisplay();
    state = reduceLocalStorageMigrationDisplay(state, {
      event: 'phase',
      data: {phase: 'verifying'},
    });
    expect(state.statusText).toBe('正在校验迁移结果…');

    state = reduceLocalStorageMigrationDisplay(state, {
      event: 'completed',
      data: {
        targetPath: 'D:/Diary/los',
        migratedFiles: 3,
        migratedBytes: 1536,
        cleanupWarning: '旧目录被占用',
      },
    });
    expect(state.completed).toBe(true);
    expect(state.fileDetail).toBe('3 个文件 · 1.5 KB');
    expect(state.cleanupWarning).toBe('旧目录被占用');
  });

  it('preserves context when a command-level error is applied', () => {
    const state = {
      ...initialLocalStorageMigrationDisplay(),
      sourcePath: 'C:/old',
    };
    const failed = withLocalStorageMigrationError(state, '磁盘空间不足');

    expect(failed.sourcePath).toBe('C:/old');
    expect(failed.statusText).toBe('本地数据迁移失败');
    expect(failed.error).toBe('磁盘空间不足');
  });
});

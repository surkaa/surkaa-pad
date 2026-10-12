import type {KeyChangeReport} from '../../../bindings';

export function reportFixture(): KeyChangeReport {
  return {
    schemaVersion: 1, runId: 'anonymous-test-run', storageMode: 'cloud', buildMode: 'debug',
    platform: 'windows', architecture: 'x86_64', startedAtMs: 1791700000000, updatedAtMs: 1791700120000,
    outcome: 'completed', attempts: 1, incompleteHistory: false,
    encryptedFiles: 5, completedFiles: 5, encryptedBytes: 104857600,
    preparationMs: 2300, derivationMs: 300, benchmarkMs: 1200, activeMs: 120000,
    readBytes: 629145600, writeStreamBytes: 104857600, cloudCopyBytes: 209715200,
    readAwaitMs: 24000, cryptoMs: 7000, hashMs: 3000,
    operations: [{name: 'readOpen', count: 30, failed: 0, totalMs: 12000, maxMs: 800}],
    phases: [{name: 'writingStage', count: 5, failed: 0, totalMs: 30000, maxMs: 12000}],
    slowestFiles: [{index: 0, kind: 'ctrAttachment', bytes: 31457280, elapsedMs: 45000, completed: true}],
  };
}

// @vitest-environment happy-dom
import {beforeEach, describe, expect, it, vi} from 'vitest';
import {createPinia, setActivePinia} from 'pinia';
import type {PendingAndroidShare} from '../../bindings';

const apiMocks = vi.hoisted(() => ({
  listPending: vi.fn<() => Promise<PendingAndroidShare[]>>(),
  acknowledge: vi.fn<(batchId: string) => Promise<void>>(),
}));

vi.mock('../../utils/api', () => ({
  default: {
    cmdListPendingAndroidShares: apiMocks.listPending,
    cmdAckPendingAndroidShare: apiMocks.acknowledge,
  },
}));

import {useAndroidShareStore} from '../androidShare';

function batch(id: string): PendingAndroidShare {
  return {id, text: `text-${id}`, items: []};
}

describe('Android share store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    localStorage.clear();
    apiMocks.listPending.mockReset();
    apiMocks.acknowledge.mockReset();
  });

  it('keeps queue reads non-destructive and creates an explicit import request', async () => {
    apiMocks.listPending.mockResolvedValue([batch('share-1'), batch('share-2')]);
    const store = useAndroidShareStore();

    await store.refresh();
    store.requestImport('share-1', 'diary-1');

    expect(store.pendingBatches).toHaveLength(2);
    expect(store.importRequest).toEqual({
      batchId: 'share-1',
      targetDiaryId: 'diary-1',
      phase: 'pending',
    });
    expect(store.importingBatch?.id).toBe('share-1');
    expect(apiMocks.acknowledge).not.toHaveBeenCalled();
  });

  it('supports the two-step existing diary selection flow', async () => {
    apiMocks.listPending.mockResolvedValue([batch('share-1')]);
    const store = useAndroidShareStore();
    await store.refresh();

    store.beginTargetSelection('share-1');
    expect(store.selectingTarget).toBe(true);

    expect(store.selectTarget('diary-2')).toBe('share-1');
    expect(store.selectingTarget).toBe(false);
    expect(store.importRequest).toEqual({
      batchId: 'share-1',
      targetDiaryId: 'diary-2',
      phase: 'pending',
    });
  });

  it('only removes a batch after the native inbox acknowledges it', async () => {
    apiMocks.listPending.mockResolvedValue([batch('share-1'), batch('share-2')]);
    apiMocks.acknowledge.mockResolvedValue();
    const store = useAndroidShareStore();
    await store.refresh();
    store.requestImport('share-1', null);
    store.markImportAwaitingAcknowledgement('share-1');

    expect(store.importRequest?.phase).toBe('acknowledging');

    await store.acknowledge('share-1');

    expect(apiMocks.acknowledge).toHaveBeenCalledWith('share-1');
    expect(store.pendingBatches.map(item => item.id)).toEqual(['share-2']);
    expect(store.importRequest).toBeNull();
  });

  it('does not create requests for stale batch ids', async () => {
    apiMocks.listPending.mockResolvedValue([]);
    const store = useAndroidShareStore();
    await store.refresh();

    expect(() => store.requestImport('missing', null)).toThrow('已经不存在');
    expect(() => store.beginTargetSelection('missing')).toThrow('已经不存在');
  });

  it('only retries acknowledgement after restart when diary content was already saved', async () => {
    apiMocks.listPending.mockResolvedValue([batch('share-1')]);
    apiMocks.acknowledge.mockResolvedValue();
    const firstStore = useAndroidShareStore();
    await firstStore.refresh();
    firstStore.requestImport('share-1', 'diary-1');
    firstStore.markImportAwaitingAcknowledgement('share-1');

    setActivePinia(createPinia());
    const restoredStore = useAndroidShareStore();
    await restoredStore.refresh();

    expect(apiMocks.acknowledge).toHaveBeenCalledWith('share-1');
    expect(restoredStore.pendingBatches).toEqual([]);
    expect(restoredStore.importRequest).toBeNull();
  });
});

import {describe, expect, it} from 'vitest';
import {LatestTaskQueue} from '../latestTaskQueue';

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => {
    resolve = done;
  });
  return {promise, resolve};
}

describe('LatestTaskQueue', () => {
  it('serializes execution and coalesces waiting values to the latest one', async () => {
    const firstStarted = deferred();
    const releaseFirst = deferred();
    const saved: number[] = [];
    let active = 0;
    let maxActive = 0;
    const queue = new LatestTaskQueue<number>(async value => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      if (value === 1) {
        firstStarted.resolve();
        await releaseFirst.promise;
      }
      saved.push(value);
      active -= 1;
    });

    queue.request(1);
    const flushing = queue.flush();
    await firstStarted.promise;
    queue.request(2);
    queue.request(3);
    expect(queue.flush()).toBe(flushing);
    releaseFirst.resolve();
    await flushing;

    expect(saved).toEqual([1, 3]);
    expect(maxActive).toBe(1);
    expect(queue.hasWork()).toBe(false);
  });

  it('retains a failed value so an explicit flush can retry it', async () => {
    let attempts = 0;
    const queue = new LatestTaskQueue<string>(async () => {
      attempts += 1;
      if (attempts === 1) throw new Error('save failed');
    });

    queue.request('content');
    await expect(queue.flush()).rejects.toThrow('save failed');
    expect(queue.hasWork()).toBe(true);
    await expect(queue.flush()).resolves.toBeUndefined();
    expect(attempts).toBe(2);
  });
});

export class LatestTaskQueue<T> {
  private pending: T | undefined;
  private running: Promise<void> | null = null;

  constructor(private readonly execute: (value: T) => Promise<void>) {}

  request(value: T): void {
    this.pending = value;
  }

  hasWork(): boolean {
    return this.pending !== undefined || this.running !== null;
  }

  flush(): Promise<void> {
    if (!this.running) {
      this.running = this.drain().finally(() => {
        this.running = null;
      });
    }
    return this.running;
  }

  private async drain(): Promise<void> {
    while (this.pending !== undefined) {
      const current = this.pending;
      this.pending = undefined;
      try {
        await this.execute(current);
      } catch (error) {
        // 执行期间若没有更新的值到来，保留失败值供下一次 flush 重试；
        // 有更新值时则只保留最新状态。
        if (this.pending === undefined) {
          this.pending = current;
        }
        throw error;
      }
    }
  }
}

type Priority = 'foreground' | 'background';
type Flight = { consumers: Set<() => void>; start?: () => void; priority: Priority };
type Request<T> = {
  signal?: AbortSignal;
  priority?: Priority;
  produce: () => Promise<T>;
  publish: (value: T) => void;
  fail: (error: unknown) => void;
  settled: () => void;
};
type Limits = { producers: number; consumers: number; queued: number };

export class ForegroundRequests {
  private flights = new Map<string, Flight>();
  private queue: { key: string; flight: Flight }[] = [];
  private consumers = 0;
  private producing = 0;

  constructor(private limits: Limits = { producers: 32, consumers: 2048, queued: 1024 }) {}

  has(key: string) { return this.flights.has(key); }
  get size() { return this.flights.size; }

  run<T>(key: string, request: Request<T>): Promise<void> {
    if (request.signal?.aborted) return Promise.resolve();
    const existing = this.flights.get(key);
    const full = this.producing >= this.limits.producers && this.queue.length >= this.limits.queued;
    if ((!existing && full) || this.consumers >= this.limits.consumers) {
      request.fail(new Error('Metadata requests are busy; retry after a current request finishes'));
      request.settled();
      return Promise.resolve();
    }
    if (existing?.start && (request.priority ?? 'foreground') === 'foreground') existing.priority = 'foreground';
    const flight = existing ?? { consumers: new Set<() => void>(), priority: request.priority ?? 'foreground' };
    this.flights.set(key, flight);
    const waiting = new Promise<void>(resolve => {
      const finish = () => {
        if (!flight.consumers.delete(finish)) return;
        this.consumers--;
        request.signal?.removeEventListener('abort', finish);
        if (flight.start && !flight.consumers.size) this.dropQueued(key, flight, request);
        resolve();
      };
      this.consumers++;
      flight.consumers.add(finish);
      request.signal?.addEventListener('abort', finish, { once: true });
    });
    if (!existing) this.admit(key, flight, request);
    return waiting;
  }

  private admit<T>(key: string, flight: Flight, request: Request<T>) {
    const start = () => {
      delete flight.start;
      this.producing++;
      void this.produce(key, flight, request);
    };
    if (this.producing < this.limits.producers) { start(); return; }
    flight.start = start;
    this.queue.push({ key, flight });
  }

  private dropQueued<T>(key: string, flight: Flight, request: Request<T>) {
    this.queue = this.queue.filter(entry => entry.flight !== flight);
    delete flight.start;
    this.flights.delete(key);
    request.settled();
  }

  private drain() {
    while (this.producing < this.limits.producers && this.queue.length) {
      const index = this.queue.findIndex(entry => entry.flight.priority === 'foreground');
      const [next] = this.queue.splice(Math.max(index, 0), 1);
      next?.flight.start?.();
    }
  }

  private async produce<T>(key: string, flight: Flight, request: Request<T>) {
    try {
      const result = await request.produce();
      if (flight.consumers.size) request.publish(result);
    } catch (error) {
      if (flight.consumers.size) request.fail(error);
    } finally {
      this.producing--;
      this.flights.delete(key);
      request.settled();
      for (const finish of [...flight.consumers]) finish();
      this.drain();
    }
  }
}

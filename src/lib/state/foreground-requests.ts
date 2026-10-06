type Flight = { consumers: Set<() => void> };
type Request<T> = {
  signal?: AbortSignal;
  produce: () => Promise<T>;
  publish: (value: T) => void;
  fail: (error: unknown) => void;
  settled: () => void;
};

export class ForegroundRequests {
  private flights = new Map<string, Flight>();
  private consumers = 0;

  constructor(private limits = { producers: 32, consumers: 256 }) {}

  has(key: string) { return this.flights.has(key); }
  get size() { return this.flights.size; }

  run<T>(key: string, request: Request<T>): Promise<void> {
    if (request.signal?.aborted) return Promise.resolve();
    const existing = this.flights.get(key);
    if ((!existing && this.flights.size >= this.limits.producers) || this.consumers >= this.limits.consumers) {
      request.fail(new Error('Metadata requests are busy; retry after a current request finishes'));
      request.settled();
      return Promise.resolve();
    }
    const flight = existing ?? { consumers: new Set<() => void>() };
    this.flights.set(key, flight);
    const waiting = new Promise<void>(resolve => {
      const finish = () => {
        if (!flight.consumers.delete(finish)) return;
        this.consumers--;
        request.signal?.removeEventListener('abort', finish);
        resolve();
      };
      this.consumers++;
      flight.consumers.add(finish);
      request.signal?.addEventListener('abort', finish, { once: true });
    });
    if (!existing) void this.produce(key, flight, request);
    return waiting;
  }

  private async produce<T>(key: string, flight: Flight, request: Request<T>) {
    try {
      const result = await request.produce();
      if (flight.consumers.size) request.publish(result);
    } catch (error) {
      if (flight.consumers.size) request.fail(error);
    } finally {
      this.flights.delete(key);
      request.settled();
      for (const finish of [...flight.consumers]) finish();
    }
  }
}

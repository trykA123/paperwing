export const REFRESH_WINDOW_MS = 300;

export type AutoRefreshHost = {
  refresh: (paths: string[]) => void;
  windowMs?: number;
};

export class AutoRefresh {
  #host: AutoRefreshHost;
  #pending = new Set<string>();
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(host: AutoRefreshHost) { this.#host = host; }

  changed(path: string) {
    this.#pending.add(path);
    this.#timer ??= setTimeout(() => this.#flush(), this.#host.windowMs ?? REFRESH_WINDOW_MS);
  }

  #flush() {
    const paths = [...this.#pending];
    this.#pending.clear();
    this.#timer = undefined;
    if (paths.length) this.#host.refresh(paths);
  }
}

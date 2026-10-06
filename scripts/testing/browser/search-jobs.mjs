import { grepRepo } from './backend.mjs';

const MAX_RUNNING = 4;
const BIG_PATTERN = 'bigtest';

/** Stands in for search_service.rs: same limit of four jobs, same three events. */
export class SearchJobs {
  constructor(emit) { this.emit = emit; this.next = 0; this.running = new Map(); this.log = []; }

  start(request) {
    if (this.running.size >= MAX_RUNNING) throw new Error('Too many searches are running; cancel one first');
    const id = ++this.next;
    const job = { cancelled: false };
    this.running.set(id, job);
    this.log.push(`start ${id}`);
    void this.#run(id, job, request);
    return id;
  }

  cancel(id) {
    const job = this.running.get(id);
    if (job) { job.cancelled = true; this.log.push(`cancel ${id}`); }
    return !!job;
  }

  cancelAll() { for (const id of this.running.keys()) this.cancel(id); return this.running.size; }

  async #run(id, job, request) {
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    await sleep(150);
    let total = 0;
    for (const target of request.repos) {
      const found = request.pattern === BIG_PATTERN ? this.#big(target.path) : grepRepo(request, target);
      let sent = 0;
      for (let at = 0; at < found.length && !job.cancelled; at += 250) {
        const batch = found.slice(at, at + 250);
        await this.emit('search-matches', { id, repo: target.path, matches: batch });
        sent += batch.length; total += batch.length;
        await sleep(request.pattern === BIG_PATTERN ? 12 : 120);
      }
      const state = job.cancelled ? 'cancelled' : 'done';
      await this.emit('search-repo', { id, repo: target.path, status: { state, matches: sent, truncated: false, error: null } });
    }
    this.running.delete(id);
    await this.emit('search-done', { id, summary: { repos: request.repos.length, matches: total, failed: 0, capped: false, cancelled: job.cancelled } });
    this.log.push(`done ${id}`);
  }

  #big(path) {
    const files = 200;
    return Array.from({ length: 3333 }, (_, index) => ({ path: `src/module-${index % files}/file-${Math.floor(index / files)}.ts`, line: (index % 90) + 1, column: 5, text: `const value${index} = compute(${index}); // ${BIG_PATTERN} in ${path.split('/').pop()}`, context: [] }))
      .sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
  }
}

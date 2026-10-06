import { grepRepo, refProblem } from './backend.mjs';

const MAX_RUNNING = 4;
const CHUNK = 200;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function synthetic(path, count) {
  const text = index => `const value${index} = compute(${index}); // bigtest in ${path.split('/').pop()}`;
  return Array.from({ length: count }, (_, index) => ({ path: `src/module-${Math.floor(index / 10)}/file-${index % 10}.ts`, line: (index % 90) + 1, column: text(index).indexOf('bigtest') + 1, text: text(index), context: [] }))
    .sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
}

/** Stands in for search_service.rs and search_job.rs: four jobs at most, 200 per repository and 2000 overall by default, matches in chunks of 200 after each repository finishes. */
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

  #found(request, target) {
    if (request.pattern === 'bigtest') return synthetic(target.path, 2000);
    if (request.pattern === 'bigone') return synthetic(target.path, 2000);
    return grepRepo(request, target);
  }

  async #run(id, job, request) {
    const big = request.pattern.startsWith('big');
    const perRepo = big ? 2000 : request.maxPerRepo ?? 200;
    const overall = request.pattern === 'bigtest' ? 10000 : request.maxOverall ?? 2000;
    const budget = { claimed: 0, capped: false };
    const summary = { repos: request.repos.length, matches: 0, failed: 0, capped: false, cancelled: false };
    await sleep(100);
    for (const target of request.repos) {
      let status, kept = [];
      const problem = refProblem(target);
      while (request.pattern === 'bighold' && !job.cancelled) await sleep(50);
      if (job.cancelled) status = { state: 'cancelled', matches: 0, truncated: false, error: null };
      else if (problem) { status = { state: 'failed', matches: 0, truncated: false, error: problem }; summary.failed += 1; }
      else if (budget.capped) { status = { state: 'skipped', matches: 0, truncated: false, error: 'Overall result limit reached' }; summary.capped = true; }
      else {
        if (big) await sleep(250);
        let truncated = false;
        for (const match of this.#found(request, target)) {
          if (kept.length >= perRepo) { truncated = true; break; }
          if (budget.claimed >= overall) { budget.capped = true; summary.capped = true; truncated = true; break; }
          budget.claimed += 1; kept.push(match);
        }
        status = { state: 'done', matches: kept.length, truncated, error: null };
      }
      const size = request.pattern === 'bigone' ? 2000 : CHUNK;
      for (let at = 0; at < kept.length; at += size) await this.emit('search-matches', { id, repo: target.path, matches: kept.slice(at, at + size) });
      summary.matches += kept.length;
      await this.emit('search-repo', { id, repo: target.path, status });
      await sleep(30);
    }
    summary.cancelled = job.cancelled;
    this.running.delete(id);
    await this.emit('search-done', { id, summary });
    this.log.push(`done ${id}`);
  }
}

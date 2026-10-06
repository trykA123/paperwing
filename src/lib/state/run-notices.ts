import type { NotificationStore } from '../notifications.svelte';

export type RunVerb = 'clone' | 'fetch' | 'pull' | 'switch' | 'push';

const WORDS: Record<RunVerb, { doing: string; done: string; name: string }> = {
  clone: { doing: 'Cloning', done: 'Cloned', name: 'Clone' },
  fetch: { doing: 'Fetching', done: 'Fetched', name: 'Fetch' },
  pull: { doing: 'Pulling', done: 'Pulled', name: 'Pull' },
  switch: { doing: 'Switching', done: 'Switched', name: 'Switch' },
  push: { doing: 'Pushing', done: 'Pushed', name: 'Push' },
};

export const doingWord = (verb: RunVerb) => WORDS[verb].doing;

const repositories = (count: number) => `${count} ${count === 1 ? 'repository' : 'repositories'}`;

export type RunResult<T> = {
  verb: RunVerb; total: number; failed: readonly T[]; firstError?: string;
  retry: (failed: readonly T[]) => void; viewActivity: () => void;
};

/** One loading notice per Git run; it turns into the outcome and offers Retry for the failed repositories only. */
export class RunNotices {
  constructor(private readonly notices: NotificationStore) {}

  begin(verb: RunVerb, total: number): number {
    return this.notices.notify(`${WORDS[verb].doing} ${repositories(total)}…`, 'loading');
  }

  finish<T>(id: number, result: RunResult<T>): void {
    const { verb, total, failed } = result;
    const words = WORDS[verb];
    const patch = failed.length
      ? {
          kind: 'error' as const, msg: `${words.name} failed for ${failed.length} of ${repositories(total)}`, detail: result.firstError,
          actions: [{ label: 'Retry', run: () => result.retry(failed) }, { label: 'View activity', run: result.viewActivity }],
        }
      : { kind: 'success' as const, msg: `${words.done} ${repositories(total)}`, detail: undefined, actions: [] };
    if (this.notices.items.some(item => item.id === id)) this.notices.update(id, patch);
    else this.notices.notify(patch.msg, patch.kind, { detail: patch.detail, actions: patch.actions });
  }
}

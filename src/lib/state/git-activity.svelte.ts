import { api, type Activity, type ActivityDelta } from '../api';
import { applyDelta, applyEntry, sortedActivity, trimActivity } from './activity-delta';

const serialOf = (id: string) => Number(id.slice(4));

export class GitActivity {
  #entries = $state.raw<ReadonlyMap<string, Activity>>(new Map());
  activity = $derived(sortedActivity(this.#entries));
  failed = $derived(this.activity.filter(entry => entry.state === 'failed' || entry.state === 'timedOut').length);
  running = $derived(this.activity.filter(entry => entry.state === 'running').length);
  #activityThrough = 0;
  #activityRetained = new Set<string>();
  #resync: Promise<void> | null = null;

  #isCleared(id: string) {
    return serialOf(id) <= this.#activityThrough && !this.#activityRetained.has(id);
  }

  mergeActivity(entry: Activity) {
    if (this.#isCleared(entry.id)) return;
    this.#entries = trimActivity(applyEntry(this.#entries, entry));
  }

  applyDelta(delta: ActivityDelta) {
    if (this.#isCleared(delta.id)) return;
    const result = applyDelta(this.#entries, delta);
    this.#entries = trimActivity(result.entries);
    if (result.resync) this.#resync ??= this.refreshActivity().finally(() => { this.#resync = null; });
  }

  async refreshActivity() {
    for (const entry of await api.activitySnapshot()) this.mergeActivity(entry);
  }

  async clearActivity() {
    const cleared = await api.clearActivity();
    this.#activityThrough = cleared.through;
    this.#activityRetained = new Set(cleared.retained);
    this.#entries = new Map([...this.#entries].filter(([id]) => !this.#isCleared(id)));
    for (const entry of cleared.running) this.mergeActivity(entry);
  }
}

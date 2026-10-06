import { api, type Activity } from '../api';

export class GitActivity {
  activity = $state<Activity[]>([]);
  #activityThrough = 0;
  #activityRetained = new Set<string>();

  mergeActivity(entry: Activity) {
    const serial = Number(entry.id.slice(4));
    if (serial <= this.#activityThrough && !this.#activityRetained.has(entry.id)) return;
    const current = this.activity.find(activity => activity.id === entry.id);
    if (current && current.sequence >= entry.sequence) return;
    this.activity = [...this.activity.filter(activity => activity.id !== entry.id), entry].sort((left, right) => left.startedAt - right.startedAt);
    while (this.activity.length > 64) {
      const index = this.activity.findIndex(activity => activity.state !== 'running');
      if (index < 0) break;
      this.activity.splice(index, 1);
    }
  }

  async refreshActivity() {
    for (const entry of await api.activitySnapshot()) this.mergeActivity(entry);
  }

  async clearActivity() {
    const cleared = await api.clearActivity();
    this.#activityThrough = cleared.through;
    this.#activityRetained = new Set(cleared.retained);
    this.activity = this.activity.filter(entry => Number(entry.id.slice(4)) > cleared.through || this.#activityRetained.has(entry.id));
    for (const entry of cleared.running) this.mergeActivity(entry);
  }

}

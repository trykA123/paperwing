import type { PullRequest, SetItem } from './api';
import { hostOfRepoId } from './host-counts';
import { PROVIDERS, providerHosts } from './modules';
import { AUTO_LOAD_LIMIT, inQueue, matchesText, queueCounts, shouldAutoLoad, type QueueId } from './pull-queue';
import { pullable, pullKey } from './pull-flow.svelte';
import type { PullKey } from './pull-support';
import { pulls } from './pulls.svelte';
import { isRepoDisabled } from './source-status';
import { scopeItems, scopeOf, unreadPaths } from './scope';
import { app } from './state.svelte';

export type PullRow = { item: SetItem; folder: string; key: PullKey; pull: PullRequest };

/** The pull request queue of the active set: the answers `pulls` already holds for its repositories. */
class PullQueue {
  queue = $state<QueueId>('open');
  query = $state('');
  page = $state(0);

  armedFor = $state<string | null>(null);

  mode = $derived(scopeOf(app.ws.shell, 'prs'));

  armKey = $derived(this.mode === 'all' ? 'all' : app.set.id);

  hosts = $derived(providerHosts(PROVIDERS.find(provider => provider.id === 'github')!, app.sources));

  /** The scope's repositories on enabled sources, narrowed to the host a flyout pick chose. */
  scoped = $derived(scopeItems(this.mode, app.set, app.ws.sets, item => app.collisionKey(app.dest(item))).filter(item => !isRepoDisabled(item.repoId, app.sources)).filter(item => !app.modules.host || hostOfRepoId(item.repoId, this.hosts) === app.modules.host));

  /** Folders of the scope whose status nothing has read yet, so their branch is unknown. */
  unread = $derived(unreadPaths(this.scoped.map(item => app.dest(item)), app.local, app.statusFailures));

  keys = $derived(pullable(this.scoped).flatMap(item => pullKey(item) ?? []));

  known = $derived(this.scoped.flatMap<PullRow>(item => {
    const key = pullKey(item);
    const entry = key ? pulls.entry(key) : undefined;
    return key && entry?.status === 'ready' && entry.pull ? [{ item, folder: app.folderOf(item), key, pull: entry.pull }] : [];
  }));

  counts = $derived(queueCounts(this.known.map(row => row.pull)));

  rows = $derived(this.known.filter(row => inQueue(this.queue, row.pull)
    && matchesText(this.query, [row.pull.title, `#${row.pull.number}`, row.pull.targetRepo, row.folder, row.key.branch])));

  /** Large sets wait for a click, because loading asks GitHub once per repository. */
  needsClick = $derived(this.keys.length > AUTO_LOAD_LIMIT && this.armedFor !== this.armKey);

  autoLoad = $derived(shouldAutoLoad({ count: this.keys.length, armed: this.armedFor === this.armKey }));

  select(queue: QueueId) { this.queue = queue; this.page = 0; }

  search(query: string) { this.query = query; this.page = 0; }

  /** The user asked for this set; later checkouts and clones load on their own. */
  load() {
    this.armedFor = this.armKey;
    return pulls.ensure(this.keys);
  }

  refresh() {
    pulls.refresh(this.keys);
    return pulls.ensure(this.keys);
  }
}

export const pullQueue = new PullQueue();

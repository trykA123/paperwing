import type { api as Api, CreatedTag, DeletedTag, PushedTag, TagInfo } from './api';
import { describeError } from './errors';

export type TagApi = Pick<typeof Api, 'localStatus' | 'listTags' | 'createTag' | 'pushTag' | 'deleteTag' | 'deleteRemoteTag'>;
/** `commit` is the full target object id; null means HEAD, which planning replaces with the commit HEAD is at now. */
export type TagTarget = { path: string; name: string; remote: string; commit: string | null };
export type PlanRow = TagTarget & { existing: TagInfo | null; error: string | null };
export type TagRequest = { name: string; message: string; push: boolean; move: boolean };

export type CreateStatus = 'created' | 'pushed' | 'push-failed' | 'refused' | 'failed';
export type CreateRow = TagTarget & { status: CreateStatus; created: CreatedTag | null; pushed: PushedTag | null; error: string | null };
export type RemoveRow = TagTarget & { status: 'removed' | 'failed'; deleted: DeletedTag | null; pushed: PushedTag | null; error: string | null };

export const shortId = (id: string | null | undefined) => (id ? id.slice(0, 8) : 'HEAD');

/** Reads each repository's tags; a failed read is kept on the row and never stops the others. */
export async function planTags(targets: TagTarget[], tag: string, api: Pick<TagApi, 'listTags' | 'localStatus'>): Promise<PlanRow[]> {
  const heads = new Map<string, string>();
  if (targets.some(target => !target.commit)) {
    try { for (const status of await api.localStatus(targets.map(target => target.path))) if (status.sha) heads.set(status.path, status.sha); } catch { /* HEAD stays implicit */ }
  }
  const rows: PlanRow[] = [];
  for (const base of targets) {
    const target = { ...base, commit: base.commit ?? heads.get(base.path) ?? null };
    try { rows.push({ ...target, existing: (await api.listTags(target.path)).find(entry => entry.name === tag) ?? null, error: null }); }
    catch (reason) { rows.push({ ...target, existing: null, error: describeError(reason, `read the tags of ${target.name}`) }); }
  }
  return rows;
}

export const moveRows = (rows: PlanRow[]) => rows.filter(row => row.existing);

/** A move may only replace a remote tag that still points at the old object, or add one the remote lacks. */
async function pushCreated(target: TagTarget, tag: string, created: CreatedTag, api: Pick<TagApi, 'pushTag'>): Promise<PushedTag> {
  if (!created.previousObject) return api.pushTag(target.path, target.remote, tag, null);
  try { return await api.pushTag(target.path, target.remote, tag, created.previousObject); }
  catch (first) {
    try { return await api.pushTag(target.path, target.remote, tag, ''); }
    catch { throw first; }
  }
}

/** One repository at a time; a failure is recorded and the loop goes on. */
export async function createTags(rows: PlanRow[], request: TagRequest, api: TagApi, onRow?: (index: number, row: CreateRow) => void): Promise<CreateRow[]> {
  const tag = request.name.trim();
  const message = request.message.trim();
  const done: CreateRow[] = [];
  for (const [index, plan] of rows.entries()) {
    const { existing: _existing, error: planned, ...target } = plan;
    const blank = { ...target, created: null, pushed: null };
    let row: CreateRow;
    if (planned) row = { ...blank, status: 'failed', error: planned };
    else if (plan.existing && !request.move) row = { ...blank, status: 'refused', error: `A tag named ${tag} already exists in ${target.name}.` };
    else {
      try {
        const created = await api.createTag(target.path, { name: tag, message: message || null, target: target.commit, moveExisting: request.move && !!plan.existing });
        row = { ...blank, created, status: 'created', error: null };
        if (request.push) {
          try { row = { ...row, pushed: await pushCreated(target, tag, created, api), status: 'pushed' }; }
          catch (reason) { row = { ...row, status: 'push-failed', error: describeError(reason, `push ${tag} from ${target.name}`) }; }
        }
      } catch (reason) { row = { ...blank, status: 'failed', error: describeError(reason, `create ${tag} in ${target.name}`) }; }
    }
    done.push(row);
    onRow?.(index, row);
  }
  return done;
}

export async function deleteLocalTags(rows: PlanRow[], tag: string, api: Pick<TagApi, 'deleteTag'>, onRow?: (index: number, row: RemoveRow) => void): Promise<RemoveRow[]> {
  const done: RemoveRow[] = [];
  for (const [index, plan] of rows.entries()) {
    const { existing: _existing, error: _error, ...target } = plan;
    let row: RemoveRow;
    try { row = { ...target, status: 'removed', deleted: await api.deleteTag(target.path, tag), pushed: null, error: null }; }
    catch (reason) { row = { ...target, status: 'failed', deleted: null, pushed: null, error: describeError(reason, `delete ${tag} in ${target.name}`) }; }
    done.push(row);
    onRow?.(index, row);
  }
  return done;
}

export async function deleteRemoteTags(targets: TagTarget[], tag: string, api: Pick<TagApi, 'deleteRemoteTag'>, onRow?: (index: number, row: RemoveRow) => void): Promise<RemoveRow[]> {
  const done: RemoveRow[] = [];
  for (const [index, target] of targets.entries()) {
    let row: RemoveRow;
    try { row = { ...target, status: 'removed', deleted: null, pushed: await api.deleteRemoteTag(target.path, target.remote, tag), error: null }; }
    catch (reason) { row = { ...target, status: 'failed', deleted: null, pushed: null, error: describeError(reason, `delete ${tag} from ${target.remote} for ${target.name}`) }; }
    done.push(row);
    onRow?.(index, row);
  }
  return done;
}

export function moveConfirmMessage(rows: PlanRow[], tag: string): string {
  const lines = rows.map(row => `${row.name}: ${shortId(row.existing?.commit)} to ${shortId(row.commit)}`);
  return `Move the tag ${tag} in ${rows.length === 1 ? '1 repository' : `${rows.length} repositories`}?\n\n${lines.join('\n')}\n\nThe old commit is no longer tagged. A pushed tag is moved on the remote only if it still points at the old commit.`;
}

export function remoteDeleteMessage(targets: TagTarget[], tag: string): string {
  const where = targets.map(target => `${target.name} (${target.remote})`).join(', ');
  return `Delete the tag ${tag} from the remote in ${where}?\n\nAnyone who has fetched the tag keeps their copy. Skein cannot restore it on the remote. Local tags are not touched.`;
}

export type RefreshDeps = {
  urlsFor: (path: string) => string[];
  ensureRefs: (urls: string[], force: boolean) => unknown;
  loadTree: (path: string, force: boolean) => unknown;
};

/** Forces the ref lists and trees to reload so pickers, caches and the drawer see the new tags. */
export function refreshAfterTagChange(paths: string[], deps: RefreshDeps) {
  const urls = [...new Set(paths.flatMap(deps.urlsFor))].filter(Boolean);
  if (urls.length) void deps.ensureRefs(urls, true);
  for (const path of new Set(paths)) void deps.loadTree(path, true);
}

import type { RepoSet } from './api';
import { confirmWith } from './confirm';
import { localOnlyNote } from './local-only';
import { hostOfItem } from './repositories';
import { app } from './state.svelte';

const hostsOf = (set: RepoSet) => set.items.map(item => hostOfItem(item, app.sources));

/** Deleting a set removes the list only; its repositories stay on disk, and nothing on a host changes. */
export async function confirmDeleteSet(set: RepoSet): Promise<void> {
  const hasFixed = set.items.some(item => item.path);
  const folders = set.items.filter(item => !item.path && app.exists[app.dest(item, set.id)]);
  const risky = folders.filter(item => { const l = app.local[app.dest(item, set.id)]; return !!l && (l.dirty > 0 || l.ahead > 0); }).length;
  const trash = app.capability('trash');
  const linux = app.nativePlatform === 'linux';
  const note = localOnlyNote(hostsOf(set));
  const recycle = linux ? 'desktop Trash' : 'Recycle Bin';
  const hint = hasFixed ? 'This set holds folders opened in place. Skein never removes those; use your file manager.'
    : !trash.supported ? trash.reason ?? 'Folder removal is unavailable.'
    : risky ? `${risky} folder(s) have uncommitted changes or unpushed commits that move with the folder.`
    : linux ? 'Shared, unsafe or changed folders stay in place.' : 'Folders used by another set, and anything that is not a Git repository, are left alone.';
  const result = await confirmWith(
    `${linux ? `Remove the set "${set.name}"? Folders stay on disk unless you choose recycling below. If any requested recycle fails, the set stays configured.` : `Delete the set "${set.name}"? The repositories stay on disk unless you also remove them below.`}\n\n${note}`,
    {
      title: linux ? 'Remove set' : 'Delete set', kind: 'warning', okLabel: linux ? 'Remove configuration only' : 'Delete', destructive: true,
      check: folders.length ? {
        label: `Also move the ${folders.length} cloned folder${folders.length === 1 ? '' : 's'} to the ${recycle}`,
        okLabel: linux ? 'Recycle folders and remove set' : undefined, disabled: !trash.supported || hasFixed, hint,
      } : undefined,
    },
  );
  if (result.accepted) await app.deleteSet(set.id, result.checked && !hasFixed);
}

import type { DrawerTarget } from './details-drawer.svelte';

export type DrawerTitle = { title: string; sub: string; label: string };

/** The words at the top of the drawer; `branch` is what the repository has checked out, when it is cloned. */
export function drawerTitle(target: DrawerTarget, facts: { folder: string; host: string; branch: string | null | undefined }): DrawerTitle {
  switch (target.kind) {
    case 'repository': return { title: facts.folder, sub: [`${facts.host} / ${target.item.org}`, facts.branch].filter(Boolean).join(' · '), label: `Quick look at ${facts.folder}` };
    case 'history': return { title: target.name, sub: facts.branch ?? '', label: `History of ${target.name}` };
    case 'commit': return { title: target.row.commit ? `Commit ${target.row.commit.short}` : target.row.label, sub: target.name, label: `Commit in ${target.name}` };
    case 'pull': return { title: `#${target.pull.number} ${target.pull.title}`, sub: target.pull.targetRepo, label: `Pull request ${target.pull.number}` };
    case 'stash': return { title: 'Stash', sub: target.name, label: `Stash in ${target.name}` };
  }
}

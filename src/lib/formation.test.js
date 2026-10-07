import { expect, test } from 'bun:test';
import { bulkTargets, filterCounts, matchesFilter, nextAction, syncView } from './formation.ts';

const repo = (extra = {}) => ({ path: '/w/a', exists: true, repo: true, branch: 'main', tag: null, sha: 'abc', upstream: 'origin/main', ahead: 0, behind: 0, dirty: 0, error: null, ...extra });
const missing = { path: '/w/m', exists: false, repo: false, branch: null, tag: null, sha: '', upstream: null, ahead: 0, behind: 0, dirty: 0, error: null };
const facts = (local, extra = {}) => ({ local, onRef: true, refLabel: 'main', fixedFolder: false, ...extra });

test('A repository that is not on disk offers Clone', () => {
  expect(nextAction(facts(missing))?.kind).toBe('clone');
});

test('Uncommitted files come first and name their count', () => {
  const action = nextAction(facts(repo({ dirty: 3, behind: 2, ahead: 1 })));
  expect(action).toMatchObject({ kind: 'commit', label: 'Commit 3 files' });
  expect(nextAction(facts(repo({ dirty: 1 })))?.label).toBe('Commit 1 file');
});

test('A repository on another branch is asked to switch before it pulls or pushes', () => {
  const action = nextAction(facts(repo({ behind: 2 }), { onRef: false, refLabel: 'release/2.4' }));
  expect(action).toMatchObject({ kind: 'switch', label: 'Switch to release/2.4' });
});

test('Diverged history is not pulled or pushed; History is the suggested action', () => {
  expect(nextAction(facts(repo({ ahead: 1, behind: 2 })))).toMatchObject({ kind: 'diverged', label: 'Diverged' });
  const rows = [{ id: 'd', facts: facts(repo({ ahead: 1, behind: 2 })) }, { id: 'b', facts: facts(repo({ behind: 2 })) }];
  expect(bulkTargets(rows, row => row.facts).behind.map(row => row.id)).toEqual(['b']);
});

test('A ref that is missing on the remote is never offered as a switch target', () => {
  expect(nextAction(facts(repo(), { onRef: false, refMissing: true }))).toBeNull();
  const rows = [{ id: 'm', facts: facts(repo(), { onRef: false, refMissing: true }) }];
  expect(bulkTargets(rows, row => row.facts).offRef).toEqual([]);
});

test('Behind pulls, ahead pushes, and an unpublished branch is published', () => {
  expect(nextAction(facts(repo({ behind: 4 })))).toMatchObject({ kind: 'pull', label: 'Pull 4' });
  expect(nextAction(facts(repo({ ahead: 2 })))).toMatchObject({ kind: 'push', label: 'Push 2' });
  expect(nextAction(facts(repo({ upstream: null })))).toMatchObject({ kind: 'push', label: 'Publish' });
});

test('A clean repository in sync, a detached head on its ref and a broken folder have no action', () => {
  expect(nextAction(facts(repo()))).toBeNull();
  expect(nextAction(facts(repo({ branch: null, upstream: null })))).toBeNull();
  expect(nextAction(facts(repo({ repo: false })))).toBeNull();
  expect(nextAction(facts(undefined))).toBeNull();
});

test('A folder opened in place is never cloned, switched or pulled', () => {
  expect(nextAction(facts(missing, { fixedFolder: true }))).toBeNull();
  expect(nextAction(facts(repo({ behind: 3 }), { fixedFolder: true, onRef: false }))).toBeNull();
  expect(nextAction(facts(repo({ ahead: 3 }), { fixedFolder: true }))?.kind).toBe('push');
});

test('Filter counts follow the same rules as the filters', () => {
  const locals = [repo({ dirty: 2 }), repo({ behind: 1, ahead: 1 }), repo({ ahead: 3 }), missing, repo(), undefined];
  expect(filterCounts(locals)).toEqual({ all: 6, changes: 1, behind: 1, ahead: 2, notCloned: 1 });
  expect(locals.filter(local => matchesFilter('ahead', local))).toHaveLength(2);
  expect(matchesFilter('notCloned', undefined)).toBe(false);
});

test('Bulk targets split a selection by what each action can do', () => {
  const rows = [
    { id: 'a', facts: facts(repo({ behind: 2 })) },
    { id: 'b', facts: facts(repo({ ahead: 1 }), { onRef: false }) },
    { id: 'c', facts: facts(missing) },
    { id: 'd', facts: facts(repo({ upstream: null }), { fixedFolder: true }) },
  ];
  const targets = bulkTargets(rows, row => row.facts);
  expect(targets.cloned.map(row => row.id)).toEqual(['a', 'b', 'd']);
  expect(targets.fetchable.map(row => row.id)).toEqual(['a', 'b']);
  expect(targets.behind.map(row => row.id)).toEqual(['a']);
  expect(targets.offRef.map(row => row.id)).toEqual(['b']);
  expect(targets.pushable.map(row => row.id)).toEqual(['b', 'd']);
});

test('The sync view reads ahead, behind and uncommitted counts', () => {
  expect(syncView(repo({ ahead: 2, behind: 1, dirty: 3 }))).toMatchObject({ kind: 'rails', ahead: 2, behind: 1, dirty: 3, inSync: false, label: '2 commits ahead, 1 commit behind, 3 uncommitted files' });
  expect(syncView(repo())).toMatchObject({ inSync: true, label: 'In sync' });
  expect(syncView(repo({ upstream: null }))).toMatchObject({ inSync: false, unpublished: true });
  expect(syncView(missing).kind).toBe('missing');
  expect(syncView(undefined).kind).toBe('unknown');
});

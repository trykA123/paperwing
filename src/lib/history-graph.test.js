import { expect, test } from 'bun:test';
import { canLoadMore, describeHistory, formatCommitDate, hasSharedBase, layoutHistory, nextRowIndex, ringLabel, ringRadius, ROW_HEIGHT } from './history-graph.ts';

const commit = (name) => ({ sha: `${name}`.padEnd(40, '0'), short: `${name}`.padEnd(8, '0').slice(0, 8), subject: `commit ${name}`, author: 'Test User', date: '2026-10-06T07:30:00+03:00' });
const history = (patch = {}) => ({
  kind: 'tracking', branch: 'main', upstream: 'origin/main', uncommitted: 0, local: [], localTotal: 0, origin: [], originTotal: 0, base: commit('base'), below: [], ...patch,
});
const kinds = (layout) => layout.rows.map((row) => row.kind);

test('diverged history stacks working tree, local, origin, base, then older commits', () => {
  const layout = layoutHistory(history({ uncommitted: 3, local: [commit('l1'), commit('l2')], localTotal: 2, origin: [commit('o1')], originTotal: 1, below: [commit('b1')] }));
  expect(kinds(layout)).toEqual(['uncommitted', 'local', 'local', 'origin', 'base', 'below']);
  expect(layout.rows.map((row) => row.y)).toEqual([16, 48, 80, 112, 144, 176]);
  expect(layout.height).toBe(6 * ROW_HEIGHT);
});

test('local commits sit on the local rail and origin commits on the origin rail', () => {
  const layout = layoutHistory(history({ local: [commit('l1')], localTotal: 1, origin: [commit('o1')], originTotal: 1 }));
  const byKind = Object.fromEntries(layout.rows.map((row) => [row.kind, row]));
  expect(byKind.local.rail).toBe('local');
  expect(byKind.origin.rail).toBe('origin');
  expect(byKind.origin.x).toBeGreaterThan(byKind.local.x);
  expect(byKind.base.rail).toBe('local');
});

test('the origin rail joins the local rail at the shared base', () => {
  const layout = layoutHistory(history({ origin: [commit('o1'), commit('o2')], originTotal: 2 }));
  const origin = layout.paths.find((path) => path.rail === 'origin');
  const base = layout.rows.find((row) => row.kind === 'base');
  expect(origin.d.startsWith('M34 16 V48 C')).toBe(true);
  expect(origin.d.endsWith(`14 ${base.y}`)).toBe(true);
});

test('an in-sync branch has the base as its only row and no rails', () => {
  const layout = layoutHistory(history());
  expect(kinds(layout)).toEqual(['base']);
  expect(layout.paths).toEqual([]);
});

test('a local-only branch has one rail, no base and HEAD on the first commit', () => {
  const layout = layoutHistory(history({ kind: 'noUpstream', upstream: null, base: null, local: [commit('l1'), commit('l2')], localTotal: 2 }));
  expect(layout.paths.map((path) => path.rail)).toEqual(['local']);
  expect(layout.rows[0].tag).toBe('HEAD');
  expect(kinds(layout)).toEqual(['local', 'local']);
});

test('capped rails get a not-shown row with the hidden count', () => {
  const layout = layoutHistory(history({ local: [commit('l1')], localTotal: 4, origin: [commit('o1')], originTotal: 2 }));
  const more = layout.rows.filter((row) => row.kind === 'more');
  expect(more.map((row) => [row.rail, row.label, row.count])).toEqual([['local', '3 local commits not shown', 3], ['origin', '1 origin commit not shown', 1]]);
});

test('an unborn repository with changes shows only the working tree ring', () => {
  const layout = layoutHistory(history({ kind: 'unborn', upstream: null, base: null, uncommitted: 1 }));
  expect(layout.rows.map((row) => [row.kind, row.label, row.count])).toEqual([['uncommitted', '1 uncommitted file', 1]]);
});

test('history summary names ahead, behind and the upstream', () => {
  expect(describeHistory(history())).toBe('In sync with origin/main');
  expect(describeHistory(history({ localTotal: 2, originTotal: 5 }))).toBe('2 ahead · 5 behind · origin/main');
  expect(describeHistory(history({ kind: 'noUpstream', upstream: null }))).toContain('No upstream');
  expect(describeHistory(history({ kind: 'detached', branch: null, upstream: null }))).toContain('Detached');
  expect(describeHistory(history({ kind: 'unborn' }))).toBe('No commits yet');
});

test('the uncommitted ring grows to fit 99+', () => {
  expect([ringLabel(7), ringLabel(120)]).toEqual(['7', '99+']);
  expect(ringRadius(120)).toBeGreaterThan(ringRadius(7));
});

test('shared base and load-more flags follow the data', () => {
  expect(hasSharedBase(history({ base: null }))).toBe(false);
  expect(hasSharedBase(history({ kind: 'noUpstream', base: null }))).toBe(true);
  expect(canLoadMore(history({ local: [commit('l1')], localTotal: 2 }))).toBe(true);
  expect(canLoadMore(history())).toBe(false);
});

test('commit dates render in Europe/Bucharest', () => {
  expect(formatCommitDate('2026-10-06T00:30:00Z')).toBe('6 Oct 2026, 03:30');
  expect(formatCommitDate('not a date')).toBe('not a date');
});

test('arrow, home and end keys move the active row within bounds', () => {
  expect(nextRowIndex('ArrowDown', 0, 3)).toBe(1);
  expect(nextRowIndex('ArrowDown', 2, 3)).toBe(2);
  expect(nextRowIndex('ArrowUp', 0, 3)).toBe(0);
  expect(nextRowIndex('Home', 2, 3)).toBe(0);
  expect(nextRowIndex('End', 0, 3)).toBe(2);
  expect(nextRowIndex('Enter', 0, 3)).toBeNull();
  expect(nextRowIndex('ArrowDown', 0, 0)).toBeNull();
});

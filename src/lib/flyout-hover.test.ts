const testModule = 'bun:test';
const { describe, expect, test } = await import(testModule);
import { HoverIntent, HOVER_DELAY, inTriangle, menuStep, onSafePath, typeaheadMatch, type Timers } from './flyout-hover';
import { awaitingByHost, hostOfRepoId } from './host-counts';

function clock() {
  let now = 0, next = 1;
  const pending = new Map<number, { at: number; run: () => void }>();
  const timers: Timers = { set: (run, ms) => { pending.set(next, { at: now + ms, run }); return next++; }, clear: id => { pending.delete(id as number); } };
  const advance = (ms: number) => {
    const end = now + ms;
    for (;;) {
      const due = [...pending.entries()].filter(([, timer]) => timer.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      pending.delete(due[0]);
      now = due[1].at;
      due[1].run();
    }
    now = end;
  };
  return { timers, advance };
}

function intent() {
  const log: string[] = [];
  const time = clock();
  const hover = new HoverIntent(time.timers, id => log.push(`open ${id}`), () => log.push('close'));
  return { hover, log, ...time };
}

const flyout = { left: 52, top: 100, right: 288, bottom: 300 };

describe('hover intent', () => {
  test('opens after the hover delay, not before', () => {
    const { hover, log, advance } = intent();
    hover.enterTrigger('github', null);
    advance(HOVER_DELAY - 1);
    expect(log).toEqual([]);
    advance(1);
    expect(log).toEqual(['open github']);
  });

  test('a pass shorter than the delay opens nothing', () => {
    const { hover, log, advance } = intent();
    hover.enterTrigger('github', null);
    advance(100);
    hover.leave({ x: 40, y: 150 }, false);
    advance(500);
    expect(log).toEqual([]);
  });

  test('closes after the pointer leaves and stays open when it returns', () => {
    const { hover, log, advance } = intent();
    hover.leave({ x: 40, y: 150 }, true);
    advance(100);
    hover.enterPanel();
    advance(500);
    expect(log).toEqual([]);
    hover.leave({ x: 100, y: 150 }, true);
    advance(HOVER_DELAY);
    expect(log).toEqual(['close']);
  });

  test('a pointer still moving toward the flyout keeps it open', () => {
    const { hover, log, advance } = intent();
    hover.leave({ x: 40, y: 150 }, true);
    for (const x of [44, 48, 50]) { advance(100); hover.move({ x, y: 160 }, flyout); }
    advance(100);
    expect(log).toEqual([]);
    advance(100);
    expect(log).toEqual(['close']);
  });

  test('a pointer moving away does not extend the wait', () => {
    const { hover, log, advance } = intent();
    hover.leave({ x: 40, y: 150 }, true);
    advance(100);
    hover.move({ x: 20, y: 700 }, flyout);
    advance(60);
    expect(log).toEqual(['close']);
  });

  test('moving to another trigger swaps the flyout', () => {
    const { hover, log, advance } = intent();
    hover.leave({ x: 40, y: 150 }, true);
    hover.enterTrigger('jira', 'github');
    advance(HOVER_DELAY);
    expect(log).toEqual(['open jira']);
  });

  test('resting on the trigger of the open flyout does nothing', () => {
    const { hover, log, advance } = intent();
    hover.enterTrigger('github', 'github');
    advance(500);
    expect(log).toEqual([]);
  });
});

describe('safe path', () => {
  test('points between the trigger and the flyout edge are on the path', () => {
    expect(onSafePath({ x: 46, y: 160 }, { x: 40, y: 150 }, flyout)).toBe(true);
    expect(onSafePath({ x: 20, y: 400 }, { x: 40, y: 150 }, flyout)).toBe(false);
    expect(onSafePath({ x: 40, y: 20 }, { x: 40, y: 150 }, flyout)).toBe(false);
  });

  test('a triangle contains its inside and not its outside', () => {
    expect(inTriangle({ x: 1, y: 1 }, { x: 0, y: 0 }, { x: 4, y: 0 }, { x: 0, y: 4 })).toBe(true);
    expect(inTriangle({ x: 4, y: 4 }, { x: 0, y: 0 }, { x: 4, y: 0 }, { x: 0, y: 4 })).toBe(false);
  });
});

describe('menu keys', () => {
  const labels = ['Pull requests', 'Actions', 'Releases', 'Pull requests', 'Actions', 'Releases'];

  test('arrows wrap and Home and End jump', () => {
    expect(menuStep('ArrowDown', 5, 6)).toBe(0);
    expect(menuStep('ArrowUp', 0, 6)).toBe(5);
    expect(menuStep('Home', 3, 6)).toBe(0);
    expect(menuStep('End', 3, 6)).toBe(5);
    expect(menuStep('x', 3, 6)).toBeUndefined();
  });

  test('one letter moves past the focused item and wraps through the host sections', () => {
    expect(typeaheadMatch(labels, 0, 'p')).toBe(3);
    expect(typeaheadMatch(labels, 3, 'p')).toBe(0);
    expect(typeaheadMatch(labels, 0, 'r')).toBe(2);
    expect(typeaheadMatch(labels, 0, 'z')).toBe(-1);
  });

  test('a longer prefix may stay on the focused item', () => {
    expect(typeaheadMatch(labels, 3, 'pull')).toBe(3);
    expect(typeaheadMatch(labels, 3, 'pp')).toBe(0);
  });
});

describe('counts per host', () => {
  const hosts = [{ host: 'github.com', sourceIds: ['s2'] }, { host: 'git.acme.example', sourceIds: ['s1'] }];
  const repoIds = new Map([['/w/a', 's1:platform/a'], ['/w/b', 's1:platform/b'], ['/w/c', 's2:acme-oss/c'], ['/w/d', 'manual:d']]);

  test('pull requests that need review count under their repository host', () => {
    expect(awaitingByHost([{ path: '/w/a' }, { path: '/w/b' }, { path: '/w/c' }], repoIds, hosts)).toEqual({ 'github.com': 1, 'git.acme.example': 2 });
  });

  test('repositories without a source, or with an unknown path, count nowhere', () => {
    expect(awaitingByHost([{ path: '/w/d' }, { path: '/nowhere' }], repoIds, hosts)).toEqual({ 'github.com': 0, 'git.acme.example': 0 });
  });
});

describe('host of a repository', () => {
  const hosts = [{ host: 'github.com', sourceIds: ['s2'] }, { host: 'git.acme.example', sourceIds: ['s1', 's3'] }];

  test('reads the source from the repository id', () => {
    expect(hostOfRepoId('s2:acme-oss/cli', hosts)).toBe('github.com');
    expect(hostOfRepoId('s3:team/api', hosts)).toBe('git.acme.example');
  });

  test('an id of no known source has no host', () => {
    expect(hostOfRepoId('s22:other', hosts)).toBeUndefined();
    expect(hostOfRepoId('manual:x', hosts)).toBeUndefined();
  });
});

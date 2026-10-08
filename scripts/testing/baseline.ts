import { strict as assert } from 'node:assert';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { identifier, launch, requireDisplay, validateProfile } from './native-profile';

const phases = new Set(['git.queue', 'git.process', 'compare.queue', 'compare.prepare', 'compare.inventory', 'compare.metadata', 'compare.history', 'ipc.open', 'ipc.refresh', 'ipc.files', 'ipc.content', 'ui.request', 'ui.files-ready', 'ui.first-render', 'ui.complete', 'editor.import', 'editor.construct', 'editor.diff']);
const operations = new Set(['other', 'version', 'rev-parse', 'status', 'ls-files', 'ls-tree', 'cat-file', 'diff', 'log', 'show', 'fetch', 'clone', 'checkout', 'switch', 'pull', 'push', 'add', 'reset', 'commit', 'branch', 'config', 'for-each-ref', 'symbolic-ref', 'rev-list', 'remote', 'stash', 'check-ignore', 'check-attr', 'search-code', 'find-file']);
export type Event = { version: 1; sample: number; phase: string; operation: string; durationMs: number };

export function parseEvents(text: string): Event[] {
  if (!text.trim()) throw new Error('No measured events');
  return text.trim().split('\n').map(line => {
    const event = JSON.parse(line);
    if (Object.keys(event).sort().join(',') !== 'durationMs,operation,phase,sample,version' || event.version !== 1 || !Number.isSafeInteger(event.sample) || event.sample < 1 || !phases.has(event.phase) || !operations.has(event.operation) || !Number.isFinite(event.durationMs) || event.durationMs < 0 || event.durationMs > 3_600_000) throw new Error('Malformed or unsafe benchmark event');
    return event;
  });
}

export function statistics(samples: number[], minimum = 20) {
  if (!Number.isInteger(minimum) || minimum < 2 || samples.length < minimum || samples.some(sample => !Number.isFinite(sample) || sample < 0)) throw new Error('Insufficient or invalid measured samples');
  const sorted = [...samples].sort((left, right) => left - right);
  const mean = samples.reduce((sum, sample) => sum + sample, 0) / samples.length;
  const variance = samples.reduce((sum, sample) => sum + (sample - mean) ** 2, 0) / (samples.length - 1);
  return { count: samples.length, p50Ms: sorted[Math.ceil(sorted.length * 0.5) - 1], p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1], meanMs: mean, varianceMs2: variance, coefficientOfVariation: mean === 0 ? 0 : Math.sqrt(variance) / mean };
}

export function summarize(events: Event[], warmups = 5, minimum = 20) {
  if (!Number.isInteger(warmups) || warmups < 0) throw new Error('Invalid warmup count');
  const groups = new Map<number, Event[]>();
  for (const event of events) { const group = groups.get(event.sample) ?? []; group.push(event); groups.set(event.sample, group); }
  const samples = [...groups.entries()].sort(([left], [right]) => left - right).slice(warmups);
  if (samples.length < minimum) throw new Error(`Need ${warmups} warmups and ${minimum} complete measured samples`);
  for (const [, group] of samples) for (const phase of ['ui.first-render', 'ui.complete']) if (group.filter(event => event.phase === phase).length !== 1) throw new Error('Sample lacks one first useful render/full completion observation');
  const totals = new Map<string, number[]>();
  const commandCounts: Record<string, number[]> = {};
  const measuredPhases = new Set(samples.flatMap(([, group]) => group.map(event => event.phase)));
  for (const phase of measuredPhases) {
    const measured = samples.map(([, group]) => group.filter(event => event.phase === phase)).filter(group => group.length);
    if (measured.length < minimum) throw new Error(`Insufficient measured samples for ${phase}`);
    totals.set(phase, measured.map(group => group.reduce((sum, event) => sum + event.durationMs, 0)));
  }
  for (const operation of operations) {
    const counts = samples.map(([, group]) => group.filter(event => event.phase === 'git.queue' && event.operation === operation).length);
    if (counts.some(Boolean)) commandCounts[operation] = counts;
  }
  return { warmups, samples: samples.length, phases: Object.fromEntries([...totals].map(([phase, samples]) => [phase, statistics(samples, minimum)])), commandCounts, commandCountsIdentical: Object.values(commandCounts).every(counts => counts.every(count => count === counts[0])), network: 'No network requested; fetch/push events require separate investigation' };
}

export function report(profile: string, cache: string, osCache: string, expensive = false) {
  if (!['app-cache-empty', 'app-cache-warm'].includes(cache) || osCache !== 'uncontrolled') throw new Error('Declare app-cache-empty/warm and OS cache uncontrolled; app restart does not prove OS-cold cache');
  const { root, metadata } = validateProfile(profile);
  const logs = join(root, 'data', identifier, 'logs');
  const events = readdirSync(logs).filter(name => /^sample-[1-9][0-9]*\.jsonl$/.test(name)).flatMap(name => parseEvents(readFileSync(join(logs, name), 'utf8')));
  const measurements = summarize(events, 5, expensive ? 10 : 20);
  const samples = [...new Set(events.map(event => event.sample))].sort((left, right) => left - right).slice(5);
  const fingerprints: string[] = [], memorySamples: number[] = [];
  for (const sample of samples) {
    const result = JSON.parse(readFileSync(join(logs, `sample-${sample}.result.json`), 'utf8'));
    if (!/^[a-f0-9]{64}$/.test(result.resultFingerprint) || result.writeFailures !== 0) throw new Error('Invalid or incomplete native result');
    for (const [operation, count] of Object.entries(result.commands)) {
      if (!operations.has(operation) || !Number.isSafeInteger(count) || (count as number) < 0 || count !== events.filter(event => event.sample === sample && event.phase === 'git.queue' && event.operation === operation).length) throw new Error('Independent command counter/trace mismatch');
    }
    fingerprints.push(result.resultFingerprint);
    const memory = JSON.parse(readFileSync(join(logs, `sample-${sample}.system.json`), 'utf8'));
    if (memory.sample !== sample || !Number.isSafeInteger(memory.peakRssBytes) || memory.peakRssBytes <= 0 || memory.polls < 1) throw new Error('Invalid native process-tree memory sample');
    if (memory.applicationCache !== cache || memory.osCache !== osCache) throw new Error('Requested cache label does not match measured sample state');
    memorySamples.push(memory.peakRssBytes);
  }
  if (new Set(fingerprints).size !== 1) throw new Error('Native result fingerprints differ across samples');
  return { version: 1, fixture: { content: metadata.content, objects: metadata.objects }, resultFingerprint: fingerprints[0], cache, osCache, measurements, memory: { unit: 'bytes', peakProcessTreeRss: statistics(memorySamples, expensive ? 10 : 20), method: 'Sampled RSS sum; shared pages may repeat and transient peaks may be missed' }, targetGate: 'Owner-observed native acceptance, hardware/revision/release flags and user-ratified fixture-specific targets remain required' };
}

function selfTest() {
  const event = (sample: number, phase = 'ui.complete', durationMs = sample): Event => ({ version: 1, sample, phase, operation: 'other', durationMs });
  for (const [phase, operation] of [['ipc.content', 'search-code'], ['ipc.files', 'find-file']]) {
    assert.equal(parseEvents(JSON.stringify({ ...event(1, phase), operation }))[0].operation, operation);
  }
  const stats = statistics(Array.from({ length: 20 }, (_, index) => index + 1));
  assert.equal(stats.p50Ms, 10); assert.equal(stats.p95Ms, 19); assert.equal(stats.meanMs, 10.5); assert.equal(stats.varianceMs2, 35);
  assert.throws(() => statistics([1, 2]));
  assert.throws(() => statistics([], 0));
  assert.throws(() => statistics(Array(20).fill(Infinity)));
  assert.throws(() => parseEvents(''));
  assert.throws(() => parseEvents('{'));
  assert.throws(() => parseEvents(JSON.stringify({ ...event(1), privateRoot: '/real' })));
  assert.throws(() => parseEvents(JSON.stringify(event(1, 'private-file'))));
  assert.throws(() => parseEvents(JSON.stringify(event(1, 'ui.complete', -1))));
  const events = Array.from({ length: 25 }, (_, index) => [event(index + 1), event(index + 1, 'ui.first-render')]).flat();
  assert.equal(summarize(parseEvents(events.map(value => JSON.stringify(value)).join('\n'))).samples, 20);
  assert.throws(() => summarize(events.filter(value => value.phase !== 'ui.first-render')));
  assert.throws(() => summarize(events.slice(0, 20)));
  assert.throws(() => summarize([...events, event(25, 'editor.import')]));
  console.log('baseline self-test passed; synthetic vectors validate parser only');
}

if (import.meta.main) {
  try {
    const args = process.argv.slice(2), [command, profile, binary] = args;
    if (command === '--self-test') selfTest();
    else if (command === '--collect' && profile && binary) {
      requireDisplay();
      const count = args.includes('--expensive') ? 15 : 25;
      for (let sample = 1; sample <= count; sample++) await launch(profile, binary, sample);
      console.log('Collected native release samples. Use --report with explicit application-cache state.');
    } else if (command === '--report' && profile) {
      console.log(JSON.stringify(report(profile, args[args.indexOf('--cache') + 1] ?? '', args[args.indexOf('--os-cache') + 1] ?? '', args.includes('--expensive')), null, 2));
    } else throw new Error('Usage: baseline.ts --collect ABS_PROFILE ABS_RELEASE_BINARY [--expensive] | --report ABS_PROFILE --cache app-cache-empty|app-cache-warm --os-cache uncontrolled [--expensive] | --self-test');
  } catch (error) { console.error(error instanceof Error ? error.message : 'Baseline operation failed'); process.exitCode = 1; }
}

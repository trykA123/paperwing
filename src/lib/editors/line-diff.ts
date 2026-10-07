const identity = (line: string) => line;

/** Edit steps Myers explores in one gap. Each step keeps 2d + 1 entries, so memory stays near 4 * limit^2 bytes (16 MB at 2000); past it the gap becomes one coarse hunk. */
const STEP_LIMIT = 2000;

export type LineHunk = readonly [aFrom: number, aTo: number, bFrom: number, bTo: number];
type MutableHunk = [number, number, number, number];

function lcsAnchors(a: Int32Array, b: Int32Array, aLo: number, aHi: number, bLo: number, bHi: number): [number, number][] {
  const countA = new Map<number, number>(), countB = new Map<number, number>(), posB = new Map<number, number>();
  for (let i = aLo; i < aHi; i++) countA.set(a[i], (countA.get(a[i]) ?? 0) + 1);
  for (let j = bLo; j < bHi; j++) { countB.set(b[j], (countB.get(b[j]) ?? 0) + 1); posB.set(b[j], j); }
  const pairs: [number, number][] = [];
  for (let i = aLo; i < aHi; i++) if (countA.get(a[i]) === 1 && countB.get(a[i]) === 1) pairs.push([i, posB.get(a[i])!]);
  const tails: number[] = [], tailIndex: number[] = [], prev = new Int32Array(pairs.length).fill(-1);
  for (let k = 0; k < pairs.length; k++) {
    let lo = 0, hi = tails.length;
    while (lo < hi) { const mid = (lo + hi) >> 1; if (tails[mid] < pairs[k][1]) lo = mid + 1; else hi = mid; }
    tails[lo] = pairs[k][1]; tailIndex[lo] = k; prev[k] = lo > 0 ? tailIndex[lo - 1] : -1;
  }
  const anchors: [number, number][] = [];
  for (let k = tailIndex.length ? tailIndex[tailIndex.length - 1] : -1; k >= 0; k = prev[k]) anchors.push(pairs[k]);
  return anchors.reverse();
}

function myers(a: Int32Array, b: Int32Array, range: MutableHunk, hunks: MutableHunk[], limit: number) {
  const [aLo, aHi, bLo, bHi] = range;
  const n = aHi - aLo, m = bHi - bLo, max = Math.min(n + m, limit), off = max + 1;
  const v = new Int32Array(2 * max + 3), trace: Int32Array[] = [];
  const column = (step: number, k: number) => k + step;
  let found = -1;
  for (let d = 0; d <= max && found < 0; d++) {
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]) ? v[off + k + 1] : v[off + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[aLo + x] === b[bLo + y]) { x++; y++; }
      v[off + k] = x;
      if (x >= n && y >= m) { found = d; break; }
    }
    trace.push(v.slice(off - d, off + d + 1));
  }
  if (found < 0) { hunks.push([aLo, aHi, bLo, bHi]); return; }
  const edits: MutableHunk[] = [];
  let x = n, y = m;
  for (let d = found; d > 0; d--) {
    const vv = trace[d - 1], k = x - y;
    const down = k === -d || (k !== d && vv[column(d - 1, k - 1)] < vv[column(d - 1, k + 1)]);
    const pk = down ? k + 1 : k - 1, px = vv[column(d - 1, pk)], py = px - pk;
    edits.push(down ? [px, py, px, py + 1] : [px, py, px + 1, py]);
    x = px; y = py;
  }
  edits.reverse();
  for (const [ax, by, ax2, by2] of edits) {
    const last = hunks.at(-1);
    if (last && last[1] === aLo + ax && last[3] === bLo + by) { last[1] = aLo + ax2; last[3] = bLo + by2; }
    else hunks.push([aLo + ax, aLo + ax2, bLo + by, bLo + by2]);
  }
}

function gap(a: Int32Array, b: Int32Array, range: MutableHunk, hunks: MutableHunk[], limit: number) {
  let [aLo, aHi, bLo, bHi] = range;
  while (aLo < aHi && bLo < bHi && a[aLo] === b[bLo]) { aLo++; bLo++; }
  while (aLo < aHi && bLo < bHi && a[aHi - 1] === b[bHi - 1]) { aHi--; bHi--; }
  if (aLo === aHi && bLo === bHi) return;
  if (aLo === aHi || bLo === bHi) { hunks.push([aLo, aHi, bLo, bHi]); return; }
  const anchors = lcsAnchors(a, b, aLo, aHi, bLo, bHi);
  if (!anchors.length) { myers(a, b, [aLo, aHi, bLo, bHi], hunks, limit); return; }
  let pa = aLo, pb = bLo;
  for (const [i, j] of anchors) { gap(a, b, [pa, i, pb, j], hunks, limit); pa = i + 1; pb = j + 1; }
  gap(a, b, [pa, aHi, pb, bHi], hunks, limit);
}

function intern(lines: readonly string[], table: Map<string, number>): Int32Array {
  const ids = new Int32Array(lines.length);
  for (let i = 0; i < lines.length; i++) {
    let id = table.get(lines[i]);
    if (id === undefined) { id = table.size; table.set(lines[i], id); }
    ids[i] = id;
  }
  return ids;
}

function mergeAdjacent(hunks: MutableHunk[]): LineHunk[] {
  const out: MutableHunk[] = [];
  for (const hunk of hunks) {
    const last = out.at(-1);
    if (last && last[1] === hunk[0] && last[3] === hunk[2]) { last[1] = hunk[1]; last[3] = hunk[3]; } else out.push([...hunk]);
  }
  return out;
}

/** Line diff: unique-line anchors first, Myers between them. `key` maps a line to its comparison form. */
export function diffLines(aLines: readonly string[], bLines: readonly string[], key: (line: string) => string = identity, limit = STEP_LIMIT): LineHunk[] {
  const table = new Map<string, number>();
  const a = intern(key === identity ? aLines : aLines.map(key), table), b = intern(key === identity ? bLines : bLines.map(key), table);
  const hunks: MutableHunk[] = [];
  gap(a, b, [0, a.length, 0, b.length], hunks, limit);
  hunks.sort((x, y) => x[0] - y[0] || x[2] - y[2]);
  return mergeAdjacent(hunks);
}

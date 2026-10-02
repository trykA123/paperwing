export type FuzzyMatch = { score: number; positions: number[] };

const BOUNDARY = new Set(['/', '_', '-', '.', ' ', ':', '@']);

// fzf-style subsequence match: tight windows, word starts and consecutive runs score higher.
function matchTerm(term: string, text: string): FuzzyMatch | null {
  const q = term.toLowerCase();
  const t = text.toLowerCase();
  let end = -1;
  for (let i = 0, j = 0; i < t.length; i++) {
    if (t[i] === q[j] && ++j === q.length) { end = i; break; }
  }
  if (end < 0) return null;
  // Walking back from the first full match finds the tightest window ending there.
  const positions: number[] = [];
  for (let i = end, j = q.length - 1; j >= 0; i--) {
    if (t[i] === q[j]) { positions.unshift(i); j--; }
  }
  let score = 0;
  positions.forEach((at, index) => {
    score += 16;
    if (at === 0) score += 12;
    else if (BOUNDARY.has(text[at - 1]) || (text[at - 1] === text[at - 1].toLowerCase() && text[at] !== text[at].toLowerCase())) score += 20;
    if (index > 0) score += positions[index - 1] === at - 1 ? 24 : -Math.min(8, at - positions[index - 1] - 1);
  });
  return { score: score - positions[0] * 0.5 - text.length * 0.1, positions };
}

// Whitespace separates terms; every term must match, like fzf.
export function fuzzy(query: string, text: string): FuzzyMatch | null {
  const terms = query.trim().split(/\s+/).filter(Boolean);
  if (!terms.length) return { score: 0, positions: [] };
  let score = 0;
  const positions = new Set<number>();
  for (const term of terms) {
    const found = matchTerm(term, text);
    if (!found) return null;
    score += found.score;
    found.positions.forEach(position => positions.add(position));
  }
  return { score, positions: [...positions].sort((a, b) => a - b) };
}

export type Segment = { text: string; hit: boolean };

export function segments(text: string, positions: number[]): Segment[] {
  if (!positions.length) return [{ text, hit: false }];
  const hits = new Set(positions);
  const out: Segment[] = [];
  for (let i = 0; i < text.length; i++) {
    const hit = hits.has(i);
    if (out.at(-1)?.hit === hit) out[out.length - 1].text += text[i];
    else out.push({ text: text[i], hit });
  }
  return out;
}

export function rank<T>(query: string, items: T[], text: (item: T) => string): { item: T; positions: number[] }[] {
  if (!query.trim()) return items.map(item => ({ item, positions: [] }));
  return items.flatMap(item => {
    const found = fuzzy(query, text(item));
    return found ? [{ item, positions: found.positions, score: found.score }] : [];
  }).sort((a, b) => b.score - a.score).map(({ item, positions }) => ({ item, positions }));
}

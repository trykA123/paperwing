import type { PullsError } from './api';

export type PullKey = { path: string; branch: string };

const ZONE = 'Europe/Bucharest';
const DAY = new Intl.DateTimeFormat('en-CA', { timeZone: ZONE });
const CLOCK = new Intl.DateTimeFormat('en-GB', { timeZone: ZONE, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
const DATE_CLOCK = new Intl.DateTimeFormat('en-GB', { timeZone: ZONE, day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });

/** Reset times come in UTC and are shown in Europe/Bucharest; the date appears only when it is not today. */
export function formatReset(iso: string, now = Date.now()): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return 'later';
  return DAY.format(at) === DAY.format(now) ? CLOCK.format(at) : DATE_CLOCK.format(at);
}

export const keyOf = ({ path, branch }: PullKey) => `${path}\u0000${branch}`;

/** The commands reject with a `PullsError` object; other failures arrive as text. */
export function readPullsError(reason: unknown): PullsError {
  if (reason && typeof reason === 'object' && 'kind' in reason && 'message' in reason) {
    const error = reason as PullsError;
    if (error.kind === 'rateLimited' && typeof error.resetAt === 'string') return error;
    return { kind: 'message', message: String(error.message) };
  }
  return { kind: 'message', message: reason instanceof Error ? reason.message : String(reason ?? 'Unknown error') };
}

export const rateLimitText = (resetAt: string, now?: number) => `GitHub rate limit reached. Pull request status resumes after ${formatReset(resetAt, now)}.`;

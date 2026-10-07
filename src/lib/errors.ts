const MAX_DETAIL = 160;

// Most specific first; the first match wins. Patterns use word boundaries so "blocked" never matches "lock".
const CAUSES: [RegExp, string][] = [
  [/command not found|is not recognized as an internal or external command|\bspawn git ENOENT\b|\bgit: not found\b|cannot find .*\bgit(\.exe)?\b/i, 'Git is not installed or not on PATH. Install Git or add it to PATH, then restart Skein.'],
  [/\b403\b[\s\S]*\b(API )?rate limit\b|\b(API|secondary) rate limit\b/i, 'GitHub’s API rate limit was reached. Wait a few minutes, then retry.'],
  [/\b(401|403)\b[\s\S]*\b(SAML|SSO)\b|\b(SAML|SSO)\b[\s\S]*\b(401|403)\b|\bSAML enforcement\b/i, 'The organization requires single sign-on: authorize the token for SSO in your GitHub token settings, then retry.'],
  [/\b(401|403)\b|\bauthentication failed\b|\bauth failed\b|\bbad credentials\b|\binvalid (token|credentials)\b|permission denied \(publickey\)/i, 'The server rejected the token. Update it in Settings > Sources.'],
  [/\b404\b|\bnot found\b|\bdoes not exist\b/i, 'The remote could not find it. Check the name and your access.'],
  [/could not resolve|connection (refused|reset|timed out)|\bnetwork\b|\btimed? ?out\b|\bunreachable\b|\boffline\b/i, 'The server did not answer. Check the network or VPN, then retry.'],
  [/permission denied|access is denied|\bread-only\b|\beacces\b/i, 'The folder is not writable. Pick another folder or fix its permissions.'],
  [/\bindex\.lock\b|\banother git process\b|\block file\b/i, 'Another Git process is using the repository. Wait for it to finish, then retry.'],
  [/not a git repository|no such file|cannot find the path/i, 'The folder is missing or is not a repository. Clone it again.'],
];

const SECRETS: [RegExp, string][] = [
  [/(https?:\/\/)[^\s/@:]+(:[^\s/@]*)?@/gi, '$1***@'],
  [/\b(gh[pousr]_[A-Za-z0-9]{8,}|github_pat_[A-Za-z0-9_]{8,}|glpat-[A-Za-z0-9_-]{8,})/g, '***'],
  [/\b(Bearer|token|Basic)\s+[A-Za-z0-9._~+/=-]{8,}/gi, '$1 ***'],
  [/\b(token|password|secret|access_token)=[^\s&]+/gi, '$1=***'],
];

export const redact = (text: string) => SECRETS.reduce((out, [pattern, replacement]) => out.replace(pattern, replacement), text);

const asText = (raw: unknown) => (raw instanceof Error ? raw.message : String(raw ?? ''));
const firstLine = (text: string) => text.split(/\r?\n/).map(line => line.trim()).find(Boolean) ?? '';

export function explainError(raw: unknown): { cause: string | null; detail: string } {
  const text = asText(raw);
  const detail = redact(firstLine(text)).replace(/^(error|fatal):\s*/i, '').slice(0, MAX_DETAIL);
  return { cause: CAUSES.find(([pattern]) => pattern.test(text))?.[1] ?? null, detail };
}

/** What went wrong, how to fix it, and always the redacted first line of the raw message. */
export function describeError(raw: unknown, action: string): string {
  const { cause, detail } = explainError(raw);
  const head = `Couldn't ${action}. ${cause ?? 'Try again.'}`;
  return detail ? `${head} Details: ${detail}` : head;
}

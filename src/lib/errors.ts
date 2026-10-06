const CAUSES: [RegExp, string][] = [
  [/\b40[13]\b|authentication|auth failed|bad credentials|token|permission denied \(publickey\)/i, 'The server rejected the credentials. Update the token in Settings > Sources.'],
  [/\b404\b|not found|does not exist|repository .* not found/i, 'The remote could not find it. Check the name and your access.'],
  [/could not resolve|connection (refused|reset|timed out)|network|timed? ?out|unreachable|offline/i, 'The server did not answer. Check the network or VPN, then retry.'],
  [/permission denied|access is denied|read-only|eacces/i, 'The folder is not writable. Pick another folder or fix its permissions.'],
  [/lock|index\.lock|another git process/i, 'Another Git process is using the repository. Wait for it to finish, then retry.'],
  [/not a git repository|no such file|cannot find the path/i, 'The folder is missing or is not a repository. Clone it again.'],
];

const firstLine = (raw: string) => raw.split(/\r?\n/).map(line => line.trim()).find(Boolean) ?? '';

/** What went wrong and how to fix it; the raw backend text only follows as a short detail. */
export function describeError(raw: unknown, action: string): string {
  const text = raw instanceof Error ? raw.message : String(raw ?? '');
  const cause = CAUSES.find(([pattern]) => pattern.test(text))?.[1];
  const detail = firstLine(text).replace(/^error:\s*/i, '').slice(0, 160);
  if (cause) return `Couldn't ${action}. ${cause}`;
  return detail ? `Couldn't ${action}. Try again. Details: ${detail}` : `Couldn't ${action}. Try again.`;
}

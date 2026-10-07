/** Every delete the app offers touches this computer only; the confirmation says so, naming the hosts involved. */
export function localOnlyNote(hosts: readonly string[]): string {
  const named = [...new Set(hosts.filter(Boolean))];
  const where = named.length === 0 ? 'the remote' : named.length === 1 ? named[0] : `${named.slice(0, -1).join(', ')} and ${named.at(-1)}`;
  return `Local only. Nothing on ${where} changes.`;
}

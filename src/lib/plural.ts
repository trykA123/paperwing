const rules = new Intl.PluralRules('en');

export function plural(count: number, noun: string, many = `${noun}s`): string {
  return `${count} ${rules.select(count) === 'one' ? noun : many}`;
}

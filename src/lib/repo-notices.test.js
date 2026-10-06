import { expect, test } from 'bun:test';
import { warningsForOrg } from './repo-notices';

test('only warnings that name this org are returned', () => {
  const warnings = ['Acme: showing 1000 of more; GitHub returned a partial page', 'other: showing 10 of more; GitHub returned a partial page'];
  expect(warningsForOrg(warnings, 'acme')).toEqual([warnings[0]]);
});

test('an org prefix must match exactly', () => {
  expect(warningsForOrg(['acme-labs: showing 1 of more; GitHub returned a partial page'], 'acme')).toEqual([]);
});

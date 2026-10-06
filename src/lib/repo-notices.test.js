import { expect, test } from 'bun:test';
import { splitListingErrors } from './repo-notices';

test('errors that name this org become its inline warnings, the rest stay errors', () => {
  const result = splitListingErrors(['Acme: rate limited', 'other: 404', 'network down'], 'acme');
  expect(result).toEqual({ orgWarnings: ['Acme: rate limited'], errors: ['other: 404', 'network down'] });
});

test('an org prefix must match exactly', () => {
  expect(splitListingErrors(['acme-labs: failed'], 'acme').orgWarnings).toEqual([]);
});

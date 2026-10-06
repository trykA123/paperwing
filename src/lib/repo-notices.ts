export type RepoNotices = { orgWarnings: string[]; errors: string[] };

/** Splits listing errors of the form "owner: reason" into warnings for this org and the rest. */
export function splitListingErrors(errors: readonly string[], org: string): RepoNotices {
  const prefix = `${org.toLowerCase()}:`;
  const orgWarnings: string[] = [], rest: string[] = [];
  for (const error of errors) (error.toLowerCase().startsWith(prefix) ? orgWarnings : rest).push(error);
  return { orgWarnings, errors: rest };
}

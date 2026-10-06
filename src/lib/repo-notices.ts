/** Listing warnings have the form "owner: message"; returns the ones that name this org. */
export function warningsForOrg(warnings: readonly string[], org: string) {
  const prefix = `${org.toLowerCase()}:`;
  return warnings.filter(warning => warning.toLowerCase().startsWith(prefix));
}

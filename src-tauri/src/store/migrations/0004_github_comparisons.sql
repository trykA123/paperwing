CREATE TABLE github_comparisons (
  id INTEGER PRIMARY KEY,
  host TEXT NOT NULL,
  owner TEXT NOT NULL,
  repository TEXT NOT NULL,
  base_sha TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  version INTEGER NOT NULL,
  body TEXT NOT NULL,
  UNIQUE (host, owner, repository, base_sha, head_sha)
) STRICT;

CREATE TABLE github_comparison_scopes (
  comparison_id INTEGER NOT NULL REFERENCES github_comparisons(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  PRIMARY KEY (comparison_id, source_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX github_comparison_sources ON github_comparison_scopes(source_id);

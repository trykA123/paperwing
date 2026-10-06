CREATE TABLE github_listings (
  source_id TEXT PRIMARY KEY,
  scope TEXT NOT NULL,
  login TEXT,
  fetched_at INTEGER NOT NULL,
  version INTEGER NOT NULL
) STRICT;

CREATE TABLE repositories (
  rowid INTEGER PRIMARY KEY,
  source_id TEXT NOT NULL REFERENCES github_listings(source_id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  id TEXT NOT NULL,
  org TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL,
  description TEXT NOT NULL,
  url TEXT NOT NULL,
  default_branch TEXT NOT NULL,
  pushed_at TEXT NOT NULL,
  archived INTEGER NOT NULL,
  fetched_at INTEGER NOT NULL,
  UNIQUE (source_id, position)
) STRICT;

CREATE TABLE commit_sets (
  id INTEGER PRIMARY KEY,
  source_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  repository TEXT NOT NULL,
  branch TEXT NOT NULL,
  ref_epoch INTEGER NOT NULL,
  fetched_at INTEGER NOT NULL,
  version INTEGER NOT NULL,
  UNIQUE (source_id, repository, branch, ref_epoch)
) STRICT;

CREATE TABLE commits (
  rowid INTEGER PRIMARY KEY,
  set_id INTEGER NOT NULL REFERENCES commit_sets(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  sha TEXT NOT NULL,
  subject TEXT NOT NULL,
  message TEXT NOT NULL,
  author TEXT NOT NULL,
  date TEXT NOT NULL,
  parents TEXT NOT NULL,
  UNIQUE (set_id, position)
) STRICT;

CREATE TABLE ref_sets (
  url TEXT PRIMARY KEY,
  scope TEXT NOT NULL,
  ref_epoch INTEGER NOT NULL,
  fetched_at INTEGER NOT NULL,
  version INTEGER NOT NULL
) STRICT;

CREATE TABLE refs (
  url TEXT NOT NULL REFERENCES ref_sets(url) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('branch', 'tag')),
  position INTEGER NOT NULL,
  name TEXT NOT NULL,
  sha TEXT,
  label TEXT,
  PRIMARY KEY (url, kind, position)
) STRICT, WITHOUT ROWID;

CREATE VIRTUAL TABLE repository_search USING fts5(
  full_name, content='repositories', content_rowid='rowid'
);

CREATE TRIGGER repositories_insert AFTER INSERT ON repositories BEGIN
  INSERT INTO repository_search(rowid, full_name) VALUES (new.rowid, new.full_name);
END;

CREATE TRIGGER repositories_delete AFTER DELETE ON repositories BEGIN
  INSERT INTO repository_search(repository_search, rowid, full_name)
  VALUES ('delete', old.rowid, old.full_name);
END;

CREATE VIRTUAL TABLE commit_search USING fts5(
  subject, content='commits', content_rowid='rowid'
);

CREATE TRIGGER commits_insert AFTER INSERT ON commits BEGIN
  INSERT INTO commit_search(rowid, subject) VALUES (new.rowid, new.subject);
END;

CREATE TRIGGER commits_delete AFTER DELETE ON commits BEGIN
  INSERT INTO commit_search(commit_search, rowid, subject)
  VALUES ('delete', old.rowid, old.subject);
END;

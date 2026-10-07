CREATE TABLE providers (
  source_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
) STRICT;

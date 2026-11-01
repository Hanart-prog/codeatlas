-- A commit's diff against its first parent never changes, so a new tip needs only
-- its new commits read from Git. The key holds what else can change the result:
-- the algorithm version, the settings and whether the clone is shallow.
CREATE TABLE commit_diffs (
    repository_id INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    commit_oid    TEXT    NOT NULL,
    cache_key     TEXT    NOT NULL,
    changes_json  TEXT    NOT NULL,
    PRIMARY KEY (repository_id, cache_key, commit_oid)
) STRICT, WITHOUT ROWID;

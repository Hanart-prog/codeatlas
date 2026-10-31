ALTER TABLE analysis_runs ADD COLUMN history_limit INTEGER NOT NULL DEFAULT 0;
ALTER TABLE analysis_runs ADD COLUMN is_shallow INTEGER NOT NULL DEFAULT 0;
ALTER TABLE analysis_runs ADD COLUMN is_truncated INTEGER NOT NULL DEFAULT 0;
ALTER TABLE analysis_runs ADD COLUMN unreadable_path_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE analysis_runs ADD COLUMN tip_file_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE analysis_runs ADD COLUMN tip_typescript_file_count INTEGER NOT NULL DEFAULT 0;

-- A different history limit indexes different commits, so it is part of the key.
DROP INDEX analysis_runs_cache_key;
CREATE UNIQUE INDEX analysis_runs_cache_key
    ON analysis_runs (
        repository_id, target_oid, parser_version, algorithm_version, config_hash, history_limit
    )
    WHERE status = 'completed';

-- Commits are immutable, so they are shared by every run of a repository.
CREATE TABLE commits (
    repository_id   INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    oid             TEXT    NOT NULL,
    committed_at    INTEGER NOT NULL,
    message_subject TEXT    NOT NULL,
    parent_count    INTEGER NOT NULL,
    PRIMARY KEY (repository_id, oid)
) STRICT;

CREATE TABLE commit_parents (
    repository_id INTEGER NOT NULL,
    child_oid     TEXT    NOT NULL,
    parent_oid    TEXT    NOT NULL,
    parent_index  INTEGER NOT NULL,
    PRIMARY KEY (repository_id, child_oid, parent_index),
    FOREIGN KEY (repository_id, child_oid)
        REFERENCES commits (repository_id, oid) ON DELETE CASCADE
) STRICT;

-- First-parent order. Timestamps cannot provide it, since they are not monotonic.
CREATE TABLE run_commits (
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs (id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    repository_id   INTEGER NOT NULL,
    commit_oid      TEXT    NOT NULL,
    PRIMARY KEY (analysis_run_id, position),
    UNIQUE (analysis_run_id, commit_oid),
    FOREIGN KEY (repository_id, commit_oid) REFERENCES commits (repository_id, oid)
) STRICT;

-- Identities depend on the walked history and on rename settings, so they
-- belong to a run.
CREATE TABLE file_identities (
    id                  INTEGER PRIMARY KEY,
    analysis_run_id     INTEGER NOT NULL REFERENCES analysis_runs (id) ON DELETE CASCADE,
    run_index           INTEGER NOT NULL,
    first_seen_position INTEGER NOT NULL,
    canonical_label     TEXT    NOT NULL,
    UNIQUE (analysis_run_id, run_index)
) STRICT;

-- Spans use first-parent positions, which are linear, unlike commit ids in a DAG.
CREATE TABLE file_paths (
    file_identity_id    INTEGER NOT NULL REFERENCES file_identities (id) ON DELETE CASCADE,
    path                TEXT    NOT NULL,
    valid_from_position INTEGER NOT NULL,
    valid_to_position   INTEGER,
    PRIMARY KEY (file_identity_id, valid_from_position)
) STRICT;

CREATE INDEX file_paths_by_path ON file_paths (path);

CREATE TABLE file_changes (
    id               INTEGER PRIMARY KEY,
    analysis_run_id  INTEGER NOT NULL REFERENCES analysis_runs (id) ON DELETE CASCADE,
    position         INTEGER NOT NULL,
    file_identity_id INTEGER NOT NULL REFERENCES file_identities (id) ON DELETE CASCADE,
    old_path         TEXT,
    new_path         TEXT,
    status           TEXT    NOT NULL
        CHECK (status IN ('added', 'modified', 'deleted', 'renamed', 'type_changed')),
    added_lines      INTEGER NOT NULL,
    deleted_lines    INTEGER NOT NULL,
    is_merge         INTEGER NOT NULL,
    exclusion        TEXT
        CHECK (exclusion IN ('binary', 'oversized', 'symlink', 'submodule', 'rule')),
    CHECK (old_path IS NOT NULL OR new_path IS NOT NULL)
) STRICT;

CREATE INDEX file_changes_by_run ON file_changes (analysis_run_id, position);
CREATE INDEX file_changes_by_identity ON file_changes (file_identity_id);

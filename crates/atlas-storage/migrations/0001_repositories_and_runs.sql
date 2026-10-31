CREATE TABLE repositories (
    id             INTEGER PRIMARY KEY,
    canonical_path TEXT    NOT NULL UNIQUE,
    head_oid       TEXT,
    created_at     INTEGER NOT NULL,
    analyzed_at    INTEGER
) STRICT;

CREATE TABLE analysis_runs (
    id                INTEGER PRIMARY KEY,
    repository_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    target_oid        TEXT    NOT NULL,
    status            TEXT    NOT NULL
        CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled')),
    started_at        INTEGER,
    completed_at      INTEGER,
    parser_version    INTEGER NOT NULL,
    algorithm_version INTEGER NOT NULL,
    config_hash       TEXT    NOT NULL,
    error             TEXT
) STRICT;

CREATE INDEX analysis_runs_by_repository ON analysis_runs (repository_id, target_oid);

-- A completed run is a cache entry: one per commit, engine version and configuration.
CREATE UNIQUE INDEX analysis_runs_cache_key
    ON analysis_runs (repository_id, target_oid, parser_version, algorithm_version, config_hash)
    WHERE status = 'completed';

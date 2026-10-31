-- Parsing depends only on content, so results are shared across commits and
-- repositories. The language is in the key because grammars differ.
CREATE TABLE parsed_blobs (
    blob_oid       TEXT    NOT NULL,
    parser_version INTEGER NOT NULL,
    language       TEXT    NOT NULL,
    parsed_json    TEXT    NOT NULL,
    PRIMARY KEY (blob_oid, parser_version, language)
) STRICT;

-- A snapshot depends on the commit, not on an analysis run, so runs can share it.
CREATE TABLE snapshots (
    id             INTEGER PRIMARY KEY,
    repository_id  INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    commit_oid     TEXT    NOT NULL,
    parser_version INTEGER NOT NULL,
    config_hash    TEXT    NOT NULL,
    coverage_json  TEXT    NOT NULL,
    UNIQUE (repository_id, commit_oid, parser_version, config_hash)
) STRICT;

CREATE TABLE snapshot_files (
    snapshot_id           INTEGER NOT NULL REFERENCES snapshots (id) ON DELETE CASCADE,
    path                  TEXT    NOT NULL,
    language              TEXT    NOT NULL,
    blob_oid              TEXT    NOT NULL,
    loc                   INTEGER NOT NULL,
    parse_status          TEXT    NOT NULL,
    external_import_count INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, path)
) STRICT;

-- source_path -> target_path means source_path imports target_path.
CREATE TABLE dependency_edges (
    snapshot_id INTEGER NOT NULL REFERENCES snapshots (id) ON DELETE CASCADE,
    source_path TEXT    NOT NULL,
    target_path TEXT    NOT NULL,
    edge_type   TEXT    NOT NULL,
    resolution  TEXT    NOT NULL,
    specifier   TEXT    NOT NULL,
    line        INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, source_path, target_path, edge_type),
    FOREIGN KEY (snapshot_id, source_path) REFERENCES snapshot_files (snapshot_id, path),
    FOREIGN KEY (snapshot_id, target_path) REFERENCES snapshot_files (snapshot_id, path)
) STRICT;

CREATE INDEX dependency_edges_by_target ON dependency_edges (snapshot_id, target_path);

CREATE TABLE unresolved_imports (
    snapshot_id INTEGER NOT NULL REFERENCES snapshots (id) ON DELETE CASCADE,
    source_path TEXT    NOT NULL,
    specifier   TEXT    NOT NULL,
    reason      TEXT    NOT NULL,
    line        INTEGER NOT NULL,
    FOREIGN KEY (snapshot_id, source_path) REFERENCES snapshot_files (snapshot_id, path)
) STRICT;

CREATE INDEX unresolved_imports_by_source ON unresolved_imports (snapshot_id, source_path);

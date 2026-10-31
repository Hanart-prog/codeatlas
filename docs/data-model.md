# Data model

The database is a rebuildable cache of derived data, stored at `<data dir>/codeatlas.sqlite` in WAL mode with foreign keys on. Tables use `STRICT` typing, and timestamps are Unix seconds.

## Migrations

Migrations live in `crates/atlas-storage/migrations` and are forward-only. Every pending migration applies in one immediate transaction, so a failure leaves the database at its previous version and concurrent starts cannot both migrate. `schema_migrations` records a checksum of each one, and opening a database fails if a shipped migration was edited or if the database came from a newer build.

Tables are added in the phase that first uses them ([ADR 0004](decisions/0004-forward-only-migrations.md)).

## Current tables

| Table | Purpose |
|---|---|
| `schema_migrations` | Applied versions, names and checksums |
| `repositories` | One row per canonical local path |
| `analysis_runs` | One analysis of a commit, with engine versions, `config_hash`, `history_limit`, status and summary counts |
| `commits`, `commit_parents` | Immutable commit facts, shared by all runs of a repository |
| `run_commits` | First-parent order for a run |
| `file_identities`, `file_paths` | Files followed through renames, with the positions each path was valid |
| `file_changes` | Every file change in a run, including excluded ones and why |

A completed run is a cache entry. A partial unique index allows one completed run per repository, target commit, parser version, algorithm version, `config_hash` and `history_limit`. Failed runs can repeat and hold no history, because history is written in one transaction.

Identities and changes belong to a run, since they depend on the walked history and on rename settings. Each new commit at the tip therefore reindexes in full until incremental indexing arrives.

Spans use first-parent positions rather than commit ids. Positions are linear, which avoids pretending that commit ids in a DAG form a time interval.

Author names and emails are not stored, because no v1 feature needs them.

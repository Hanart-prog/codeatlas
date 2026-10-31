# Architecture

Status: Phase 4.

```text
Local repository (read-only)
        |
   atlas-git ------- atlas-parser, atlas-graph
        |                  |
        +-----> atlas-analysis
                        |
                  atlas-storage -- SQLite in the OS cache directory
                        |
        +---------------+---------------+
        |                               |
    atlas-cli  ----- starts ---->   atlas-api (127.0.0.1)
                                        |
                                   apps/web (React)

atlas-model: shared by every crate
```

## Crates

| Crate | Responsibility | Status |
|---|---|---|
| `atlas-model` | Identifiers, DTOs, configuration, error model. No Git, SQLite or HTTP | Working |
| `atlas-git` | Read-only commits, first-parent walks, diffs, renames, exclusions | Working |
| `atlas-analysis` | Identity lineage, churn, the indexing pipeline, snapshots, read models, co-change, impact ranking, architecture comparison | Working; backtests to come |
| `atlas-storage` | SQLite connection, migrations, history persistence | Working |
| `atlas-api` | HTTP translation, background jobs, request guard, static dashboard | Working |
| `atlas-cli` | The `codeatlas` binary | `analyze`, `overview`, `graph`, `impact`, `compare` and `serve` working; the rest report their phase |
| `atlas-parser` | TS/TSX import extraction, resolution, `tsconfig` aliases | Working |
| `atlas-graph` | Directed graph, cycles, reverse reachability | Working |
| `atlas-fixtures` (`fixtures/generator`) | Deterministic fixture repositories, for tests only | Working |

Integration tests live beside the crates they exercise, in `crates/*/tests`.

The engine does not depend on the API or the frontend, so it works without React. The frontend contains no analysis logic.

## Conventions

- **Edge direction:** `A -> B` means file `A` imports file `B`. Reverse traversal from `B` finds files that `B` may impact.
- **Evidence stays separate:** `DependencyEdgeType` covers static evidence only. Co-change is correlation and has its own types, fields and display.
- **Comparison:** two commits are diffed from their snapshots, with base paths carried through renames first, so a rename alone changes no import or cycle ([ADR 12](decisions/0012-compare-trees-directly.md)). The timeline reads the same snapshots at evenly spaced commits.
- **No look-ahead:** impact walks Git from the cutoff commit and reads the snapshot of that commit, so later history is never read ([ADR 11](decisions/0011-compute-impact-from-the-cutoff.md)).
- **Output streams:** results go to stdout, diagnostics to stderr.
- **Shared types:** Rust DTOs generate the TypeScript in `apps/web/src/types/generated`. A test fails when the two drift ([ADR 0005](decisions/0005-generated-api-types.md)).

## Trust boundaries

Repository content, Git metadata and filenames are untrusted.

- `Oid` and `RepoPath` can only be built by parsing, which rejects malformed ids and paths that escape the repository.
- Configuration comes from the user, never from the analyzed repository.
- The API has no authentication. It binds to loopback only and rejects requests whose `Host` or `Origin` shows they came through another site ([ADR 0002](decisions/0002-local-only-api.md)).
- CodeAtlas data lives outside analyzed repositories ([ADR 0001](decisions/0001-data-outside-repositories.md)).
- `unsafe` code is forbidden across the workspace.
- `git2` is built without network features, so OpenSSL and libssh2 are not linked.
- Repositories open without searching parent directories, and tests prove analysis leaves every file byte-identical and runs no hooks.
- Repository config cannot change rename detection, because the find flags are explicit.
- Source files are parsed, never evaluated. Snapshots read Git blobs at the commit, so working tree files cannot leak in, and a cursor walk keeps deep nesting from overflowing the stack.
- Served pages carry a Content Security Policy that blocks inline scripts and outside connections.
- Commit subjects and CLI text output replace control characters, which blocks terminal escape injection. The dashboard renders all repository text as text.

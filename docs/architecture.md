# Architecture

Status: Phase 1. Crates marked *reserved* contain no behaviour yet.

```text
Local repository (read-only)
        |
   atlas-git ------- atlas-parser (reserved)
        |                  |
        +-----> atlas-analysis, atlas-graph (reserved)
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
| `atlas-analysis` | Identity lineage, churn, the indexing pipeline, the overview | Working; co-change, impact and backtests to come |
| `atlas-storage` | SQLite connection, migrations, history persistence | Working |
| `atlas-api` | HTTP translation, background jobs, request guard | Working |
| `atlas-cli` | The `codeatlas` binary | `analyze`, `overview` and `serve` working; the rest report their phase |
| `atlas-parser`, `atlas-graph` | Imports and graphs | Reserved |
| `atlas-fixtures` (`fixtures/generator`) | Deterministic fixture repositories, for tests only | Working |

Integration tests live beside the crates they exercise, in `crates/*/tests`.

The engine does not depend on the API or the frontend, so it works without React. The frontend contains no analysis logic.

## Conventions

- **Edge direction:** `A -> B` means file `A` imports file `B`. Reverse traversal from `B` finds files that `B` may impact.
- **Evidence stays separate:** `DependencyEdgeType` covers static evidence only. Co-change is correlation and gets its own storage and display.
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
- Commit subjects and CLI text output replace control characters, which blocks terminal escape injection. The dashboard renders all repository text as text.

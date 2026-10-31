# Algorithms and metric definitions

Status: Phase 2. Later phases add co-change, impact scoring and backtesting.

## History

The **indexed history** is the first-parent chain from the analyzed commit, limited to the newest `history_limit` commits (default 500). Positions count from 0 at the oldest indexed commit.

Each commit is one **change set**: its diff against its first parent, or against an empty tree for a root commit. A merge is therefore one change set holding everything it brought in, which lets repositories that merge pull requests be analyzed without double counting.

## Renames

Renames come from libgit2 similarity detection at `analysis.rename_similarity_percent` (default 50), with up to 1000 candidates per commit. Results can differ from the `git` CLI near the threshold.

## File identity

An identity follows one file across renames along the chain:

- A rename continues the identity at a new path, and swaps within one commit keep each file's identity.
- A deletion ends the identity. A new file at that path is a new identity.
- A file first seen by modification or rename started before the window, and its identity starts at that position.

Each identity records the first-parent positions where each path was valid.

## Exclusions

A file change is excluded from metrics, but stored and counted, for the first reason that applies:

| Reason | Meaning |
|---|---|
| `symlink`, `submodule` | The entry has no source content |
| `rule` | Either path matches `analysis.exclude_globs` |
| `oversized` | The blob exceeds `limits.max_file_bytes` (default 2 MiB) |
| `binary` | Git classified the content as binary |

Changes whose paths are not valid UTF-8 cannot be stored and are only counted, as `unreadable_path_count`.

## Churn

For each identity, within the indexed history and over non-excluded changes:

| Metric | Unit | Definition |
|---|---|---|
| `commit_count` | commits | Distinct commits with a change to the identity, including pure renames |
| `added_lines` | lines | Sum of lines added, as Git counts them |
| `deleted_lines` | lines | Sum of lines deleted, as Git counts them |

A rename contributes only its line edits. Results are ordered by `commit_count` descending, then path ascending. The overview lists the top 20.

Churn describes activity. It says nothing about defects or about the people who made the changes.

## Imports

Imports are read from Git blobs with Tree-sitter, using the TypeScript grammar for `.ts`, `.mts` and `.cts` and the TSX grammar for `.tsx`.

| Edge type | Source form |
|---|---|
| `static_import` | `import ... from`, `import '...'`, `import x = require('...')` |
| `type_only_import` | `import type`, `export type ... from`, or `import { type A }` with only type specifiers |
| `re_export` | `export ... from` |
| `dynamic_import` | `import('...')` with a literal path |

## Resolution

`A -> B` means file `A` imports file `B`. A specifier resolves in this order:

1. Relative paths join the importer's directory. Paths leaving the repository are rejected.
2. `tsconfig` `paths` entries match by longest prefix. A match never falls through to a package.
3. `tsconfig` `baseUrl` is tried for bare paths.
4. `@/x` and `~/x` with no definition are `unknown_alias`. Other bare specifiers are external.

Candidates are the exact path, ESM extension mappings (`.js` to `.ts` or `.tsx`, `.jsx` to `.tsx`, `.mjs` to `.mts`, `.cjs` to `.cts`), then `.ts`, `.tsx`, `/index.ts` and `/index.tsx`. Only supported, non-excluded source files can be targets, and anything else gets a reason instead of an edge.

## Coverage

| Field | Definition |
|---|---|
| `parsed_file_count` | Source files with a syntax tree, including `partial_file_count` with syntax errors |
| `skipped_file_count` | Source files over a limit, unparseable, or LFS pointers |
| `resolved_import_count` | Import statements resolved to a local file. Several can share one edge |
| `external_import_count` | Import statements classified as packages |
| `unresolved_import_count` | Import statements that are neither, by reason |

The share of local imports resolved is `resolved / (resolved + unresolved)`.

## Graph metrics

- **Degrees** count distinct files, whatever the number or kind of imports between a pair.
- **Cycles** are strongly connected components with more than one file, plus self-imports. Member lists are sorted. A cycle is `type_only` if none of its files is in a cycle without `import type` edges, since such edges are erased at compile time.
- **Reverse reachability** is a breadth-first search over incoming edges, giving each dependent's shortest distance. Phase 3 uses it.

## Verification

Besides the fixture, Phase 1 was checked against the `git` CLI on `sindresorhus/ky` (592 first-parent commits, one manual run, not part of CI):

- Commit, merge and tree file counts matched.
- 9 of the 10 most changed files matched `git log --follow --numstat` exactly on commits and lines.
- The 10th differed because `git` scored a rename at 47% and libgit2 at 50% or more. Its totals reconcile exactly with a two-path `git log`.
- Whole-history file-change rows reconcile: 1,914 against 1,917, explained by 3 extra renames that each merge a delete and an add.

Phase 2 edges were compared with the TypeScript compiler's `resolveModuleName`, in a manual run outside CI, on three repositories:

| Repository | Files | Edges | Result |
|---|---:|---:|---|
| `sindresorhus/ky` | 98 | 182 | Identical sets |
| `shadcn-ui/taxonomy` (uses `@/` aliases) | 123 | 247 | Identical sets |
| `colinhacks/zod` (monorepo) | 515 | 640 | Every edge resolves the same when given to the compiler |

For `zod`, the check script found 634 edges because TypeScript's `preProcessFile` scanner skipped 6 `export * as x from` lines. The Phase 2 `zod` figures exclude its docs package's own aliases, which are out of scope and reported as `unknown_alias`.

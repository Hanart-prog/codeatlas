# Algorithms and metric definitions

Status: Phase 3. Phase 5 adds backtesting.

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

## Co-change

History is the `history_window` first-parent commits (default 200) ending at the cutoff, walked from the cutoff itself.

A file change is **eligible** if it is not excluded and its path is TS/TSX, the same files the import graph covers. A commit's set is its distinct eligible file identities, so renames stay one file and a commit counts once per file. Commits with an empty set, or more files than `bulk_commit_file_threshold` (default 50), are skipped and reported. Merges count once, against their first parent. A pair needs `min_shared_commits` (default 3) to count.

| Metric | Definition |
|---|---|
| `shared` | Eligible commits touching both files |
| `P(B changed given A changed)` | `shared / count(A)`. Directional |
| Jaccard | `shared / (count(A) + count(B) - shared)` |
| Base rate | `count(B) / eligible commits` |
| Lift | `P(B given A) / base rate`. Near 1, B changes this often anyway |

Pairs are counted sparsely per commit, so work is bounded by the bulk threshold rather than the number of file pairs.

## Change impact

Inputs are the selected files as they exist at the cutoff, a depth (default 3), a window, and weights. For each candidate file:

- `static_score = 1 / distance`, using the minimum reverse dependency distance to any selected file within depth, otherwise 0. Type imports count.
- `historical_score` is the maximum `P(candidate given selected)` over selected files with enough shared commits, otherwise 0.
- `score = 0.6 * static_score + 0.4 * historical_score`.

Results sort by score descending, then path ascending, comparing scores at 6 decimal places so that rounding noise cannot decide an order. Selected files and files deleted by the cutoff are never recommended. The score is a ranking heuristic, **not a probability of failure**.

A selected file must exist at the cutoff. Its identity is resolved at that point, so history follows renames made before the cutoff, and later renames are unknown.

## Architecture comparison

Each side is the import snapshot of one commit. Comparing any two commits works, and nothing is stored, because identical inputs reproduce identical results.

- **Renames** come from a direct diff of the two trees at `rename_similarity_percent`, then from replaying file identities along the first-parent chain between them, when one commit is on the other's chain within 2,000 commits. A file added after the earlier commit never links to a deleted file that had its path, and a file deleted later is not followed. One counts only if the old path left the graph and the new path is new to it, and each path has one claim, with direct evidence first. Base paths are then carried to their new names before anything else is compared.
- **Files:** added files are only at the target and removed files only at the base, leaving out renames.
- **Imports** are identified by source, target and kind, so a moved line or a rewritten specifier is not a change. Imports that differ only through a rename are counted, not listed.
- **Cycles** are strongly connected components. The same member set is the same cycle; one that changes between type-only and runtime stays unchanged, with a note. Remaining cycles that share members pair up, largest overlap first, as a changed cycle with the files that joined and left. Anything unpaired is new or resolved.
- **Metrics** are listed for files at both commits when in-degree, out-degree or transitive dependents differ, largest total change first, then path. Dependents count every file that reaches this one through imports, including through type imports, and leave the file itself out. Only files reachable from a changed import's target can have different dependents, so others skip the count; a property test checks this against a full recount.
- **`uncertain`** marks an added or removed import whose importing file did not parse cleanly at either commit, because the difference may come from parsing.
- **Notes** say why figures may not compare like with like: the commits are diverged, backward or equal; the root `tsconfig.json` differs; parsing was incomplete; or unresolved or invisible import counts differ.

`limit` cuts each list (default 500). Counts are always the true totals.

**Timeline:** file, import and cycle counts at evenly spaced commits of the latest analysis, always with the first and last, each read from that commit's own snapshot.

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

Phase 3 co-change counts were recomputed from `git log --name-status` with no libgit2 involved, in a manual run outside CI. On `sindresorhus/ky`, all 35 pairs for `source/core/Ky.ts` matched on selected, candidate and shared counts, and the eligible commit count matched (176). On `shadcn-ui/taxonomy` the eligible count matched (64). Scores themselves have not been evaluated against outcomes; Phase 5 does that.

Phase 4 comparisons are recomputed by [scripts/verify_compare.py](../scripts/verify_compare.py), which shares no code with the engine. It reads each commit's edges from `codeatlas graph` and works out differences, Tarjan's cycles, cycle pairs, degrees and dependents itself, on 20 commit pairs per repository, and `git diff -M50%` checks direct renames. CI runs this on a pinned `ky` commit through `make verify`, along with [scripts/verify_imports.cjs](../scripts/verify_imports.cjs), which repeats the Phase 2 compiler check. The other two repositories were run by hand.

| Repository | Pairs | Import changes | File changes | Cycle changes | Metric changes | Result |
|---|---:|---:|---:|---:|---:|---|
| `sindresorhus/ky` | 20 | 2,121 | 958 | 25 | 154 | Identical |
| `shadcn-ui/taxonomy` | 20 | 2,266 | 905 | 0 | 419 | Identical |
| `colinhacks/zod` (last 402 commits) | 20 | 1,413 | 988 | 33 | 1,631 | Identical |

Direct renames matched `git` on all of `ky` and `zod`, and differed on 8 `taxonomy` pairs because libgit2 and `git` score similarity differently. History following added 12 renames on `taxonomy` that the direct diff missed. The script uses the engine's renames for its import figures, so rename differences stay separate. It also shares the parser's edges, which is why the compiler check runs beside it.

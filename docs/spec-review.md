# Specification review

A review of `CodeAtlas_Claude_Master_Spec.md` v1.0, found while building Phases 0 and 1. The specification stays authoritative; where this document records a resolution, the resolution overrides it.

The two gaps most likely to undermine the project are [the merge policy (1)](#1-the-merge-policy-can-erase-the-history-of-merge-based-repositories) and [import resolution on real codebases (2)](#2-the-resolver-would-miss-most-imports-in-real-typescript-projects).

## Already fixed in Phase 0

| Section | Problem | Resolution |
|---|---|---|
| 3 | `.codeatlas/` inside a repository conflicts with the read-only rule | Data defaults to the OS cache ([ADR 1](decisions/0001-data-outside-repositories.md)) |
| 3, 12 | Loopback plus CORS leaves the API open to DNS rebinding and cross-site POSTs from the user's browser | `Host` and `Origin` checks; non-loopback binding refused ([ADR 2](decisions/0002-local-only-api.md)) |
| 5, 16 | `config_hash` sits on `repositories`, but section 16 caches by commit and configuration, so one repository needs several | Moved to `analysis_runs`, with a unique cache key on completed runs |
| 2 | No Node version is stated, and current tools exclude Node 20.16 | Versions pinned ([ADR 3](decisions/0003-frontend-toolchain-versions.md)) |
| 3 | Nothing says where configuration may come from | User only, never the analyzed repository; unknown keys are errors |

## Correctness gaps to resolve

### 1. The merge policy can erase the history of merge-based repositories

Sections 5 and 7 say to use first-parent history and to exclude merges. In a repository that merges pull requests with merge commits, the first-parent chain on `main` is almost entirely merges, so excluding them leaves little or nothing to analyze.

**Resolved in Phase 1:** walk the first-parent chain and treat each commit, merge or not, as one change set diffed against its first parent. A merged PR then counts once, which also meets the rule against double counting. Report the share of merge commits, because one merge can bundle unrelated work and the bulk threshold will filter large ones.

### 2. The resolver would miss most imports in real TypeScript projects

Section 6 resolves only relative paths to `.ts`, `.tsx`, `/index.ts` and `/index.tsx`. Real projects also use forms it leaves out:

- ESM TypeScript writes `import './a.js'` for a file named `a.ts`.
- `.mts` and `.cts` files exist.
- Literal `import('./x')` is common for lazy routes, and the specification neither supports nor excludes it.
- Many applications import through a `tsconfig` alias such as `@/lib/x`.

The fixture would pass while a real repository produced a sparse graph, leaving impact rankings and backtests with little static evidence.

**Resolved in Phase 2:** add `.js`, `.jsx`, `.mjs` and `.cjs` specifier mapping, `.mts` and `.cts` files, and literal dynamic imports as a distinct edge type. Coverage reports resolved and unresolved counts with reasons; see [algorithms.md](algorithms.md).

**Decided:** limited `tsconfig` `paths` and `baseUrl` support moves into v1, in Phase 2. It reads only the root `tsconfig.json` from the Git blob at each commit, with no `extends` chains or monorepo workspaces ([ADR 9](decisions/0009-tsconfig-paths-in-v1.md)).

### 3. Backtest definitions leave results open to interpretation

Resolved in Phase 5 as below ([evaluation.md](evaluation.md)).

| Gap in section 10 | Resolution (Phase 5) |
|---|---|
| Files added by the evaluation commit did not exist at the parent, so they can never be recommended | Exclude them from targets and report the count |
| "Eligible file" is undefined; co-change may include Markdown or lockfiles while the graph has only TS | Define eligibility once: TS/TSX, not excluded by rules. Use it for inputs, targets and co-change, and report what it removes |
| `precision@k` offers two denominators | Always `hits / k`, and report the mean candidate count beside it |
| The smallest path always becomes the input, which favours certain directories | Keep it as the primary rule; add a deterministic seeded random selection as a sensitivity check |
| Baselines cannot show what the static signal adds | Add a reverse-dependency-only baseline to the two specified, and a seeded random baseline |

### 4. Impact scoring is underspecified and structurally favours static edges

- Section 9 asks for best-effort rename resolution without saying how, and a first-parent lineage cannot relate commits on different lines. **Resolved in Phase 4:** combine a tree diff with first-parent identity replay ([ADR 12](decisions/0012-compare-trees-directly.md)).
- Section 9 says to show why a metric may be incomparable but gives no rule. **Resolved in Phase 4:** imports from files that did not parse cleanly are marked `uncertain`, and notes cover `tsconfig.json`, parsing, resolution and commit order.
- Section 9 compares cycles by member set, which reports a cycle gaining one file as two unrelated events. **Resolved in Phase 4:** exact sets still decide unchanged cycles, and leftovers sharing members are paired as a changed cycle.
- The specification has no timeline. **Added in Phase 4:** it answers which two commits are worth comparing, and it uses only existing snapshots.
- Section 12 lists `/backtests` with no way to read a result. **Resolved in Phase 5:** `GET /repositories/:id/backtests` returns `{ entry }`, and jobs report progress.
- Section 8 gives `static_score = 1 / distance` without saying how to combine several selected files. **Resolved in Phase 3:** use the minimum distance across selected files, matching `max_over_selected` for history.
- **Resolved after Phase 5:** backtesting showed the specified score losing to a baseline, so [ADR 14](decisions/0014-score-from-smoothed-co-change.md) replaced it.
- With weights 0.60 and 0.40, any direct importer with no history (0.60) outranks a file with `P = 1.0` and no import (0.40). Keep the weights as specified, and use the Phase 5 backtest to report how alternatives perform.
- Conditional probability inflates files that change constantly, such as barrels and changelogs. **Resolved in Phase 3:** base rate and lift are shown for explanation only, and a low lift adds a caveat.
- Floating-point ties can differ by rounding. **Resolved in Phase 3:** scores compare at fixed precision before the path tiebreak.

### 5. Git edge cases not covered

Resolved in Phase 1, each with a test:

- Non-UTF-8 paths are counted and skipped, never replaced lossily.
- Symlinks and submodules are excluded from metrics and counted.
- The rename threshold is a setting, `analysis.rename_similarity_percent`, included in `analysis_hash`.
- History is ordered by first-parent position, never by timestamp.
- Shallow history is detected and shown as a warning.

Phase 2 skips Git LFS pointers when parsing.

Building Phase 1 also found one thing the specification should warn about: the history limit changes results, so it belongs in the cache key alongside `config_hash`.

## Visual design

Section 13 asks for a dark-first interface. **Changed after Phase 6:** the dashboard follows the Micro style and opens in light, with a dark theme one click away ([ADR 19](decisions/0019-micro-visual-style.md)).

## Data model changes

Phases 1 and 2 applied the `run_commits`, `commits`, `snapshots` and `parsed_blobs` changes, and placed identities on the run. The rest apply as each table is introduced.

| Table | Problem | Change |
|---|---|---|
| `cochange_edges` | One materialized set per cutoff is too large for a backtest with hundreds of cutoffs | Resolved in Phase 3: computed by identity from Git at each cutoff, not stored ([ADR 11](decisions/0011-compute-impact-from-the-cutoff.md)) |
| `impact_runs`, `impact_results` | Deterministic results can be recomputed, so storage adds only a staleness risk | Not created; reports did not need them |
| `snapshots` | Tied to `analysis_run_id`, which blocks section 16's reuse across runs | Key by repository, commit, parser version and configuration hash |
| New `parsed_blobs` | Section 16 caches by blob, but no table exists | Add, keyed by blob OID and parser version |
| New `jobs`, `reports` | Sections 12 and 13 use them, with no table | Not created: jobs live in memory and reports are built on request ([ADR 17](decisions/0017-reports-are-built-on-request.md)) |
| New `run_commits` | First-parent order is needed everywhere and must be stable | Store run, position and OID |
| `commits` | Stores plain `author_name` next to a hashed email, and an unsalted email hash is easily reversed. No v1 feature needs either | Drop both unless the optional contributor view is built |

## API gaps

| Issue | Change |
|---|---|
| `/files/:encoded_path` contradicts section 12's own warning about path routes | `GET /repositories/:id/files?path=` |
| Jobs are cancellable, but no route cancels them | Resolved in Phase 6: `POST /jobs/:id/cancel` |
| Overview needs recent jobs, and the file picker needs search, but neither has a route | Resolved in Phase 6: `GET /repositories/:id/jobs`, and `search` on `/graph` |
| Reports can be downloaded but not created | Resolved in Phase 6: `GET /repositories/:id/report` builds one on request |
| The dashboard cannot list repositories it has opened | `GET /repositories` |
| Explorer toggles co-change edges, but `/graph` did not return them | Resolved in Phase 3: `include=cochange` returns them in a separate array |
| POST bodies | Require `application/json` and set a body size limit, which also reinforces ADR 2 |

## Planning gaps

- **Serving the dashboard:** resolved in Phase 2. `codeatlas serve` serves `apps/web/dist` from disk rather than embedding it ([ADR 10](decisions/0010-serve-dashboard-from-disk.md)).
- **Parse caching came too late:** resolved in Phase 2 with `parsed_blobs`, ahead of the Phase 5 backtest that parses a snapshot for every case.
- **Fixture generator:** done in Phase 1 as `atlas-fixtures`, a Rust crate that is the only writer of repositories in the workspace, with fixed dates so object ids are reproducible.
- **`.ts` and `.tsx` need separate grammars**, since `<T>x` parses differently. Phase 2 tests this, including in the parse cache.
- **UI verification:** resolved in Phase 2 with a Playwright flow in CI. It was needed: it found a first-run Analyze bug that unit tests and a manual check had missed.
- **Default excludes:** add lockfiles once non-TS files are analyzed.
- **Repository access:** "local configured workspace" in section 12 is undefined. For v1, treat it as any path the local user explicitly opens, and record that in `limitations.md` when Phase 1 builds it.
- **The specification itself** is not in the repository, although its prompt expects to read it there. It is written as agent instructions, so a public repository may prefer it under `docs/`, renamed.

# Limitations

## Current state

- Rankings were measured on six held-out repositories, which is a small sample. See [evaluation.md](evaluation.md).
- Benchmarks come from one machine and a few repositories. See [benchmarks.md](benchmarks.md).
- `codeatlas serve` serves the dashboard from `apps/web/dist`, so a binary alone serves only the API, and it must run from the repository root or be given `--web-dir`.
- The dashboard is designed for desktop windows. It was checked at 768px wide and above, and phones are not supported.
- Only macOS arm64 has been tested locally, and Linux runs in CI. Windows is untested.
- Stopping takes effect at the next step, so one huge diff runs to its end first ([ADR 16](decisions/0016-stop-work-through-the-database-session.md)). Requests stop after `server.query_timeout_seconds` (default 120). A browser that disconnects does not stop its request sooner.
- Jobs live in memory, so job ids and the Recent jobs list do not survive a restart. Results do.
- Each new tip stores a full copy of its window, and old runs are never removed. Deleting the database is safe, because everything in it can be rebuilt.
- `make verify` and the CI `verify` job recheck one pinned commit of `sindresorhus/ky` on each push. `shadcn-ui/taxonomy` and `colinhacks/zod` are rechecked by hand.
- A process killed mid-analysis leaves its run marked `running`. It holds no history and does not block a later run. Diffs it had already saved are reused.
- The demo repository is generated. Its numbers show how the tool works and say nothing about real projects.

## History semantics

- Only first-parent history is analyzed. Commits that exist only on merged branches are not counted individually, and a merge counts as one change set against its first parent. One merge can bundle unrelated work.
- Metrics cover the indexed window (default 500 commits). Older commits are not counted; the overview warns when the limit applied. A file first seen by modification has older history outside the window.
- Shallow clones are detected and flagged, and their metrics are incomplete.
- Renames are a similarity estimate. libgit2 and the `git` CLI score similarity differently, so files near the 50% threshold can differ: on one real repository, CodeAtlas found 13 renames and `git log -M50%` found 10. Commit and line totals agree once those renames are accounted for.
- A file deleted and later recreated at the same path is two identities, even if the content matches.
- Paths that are not valid UTF-8 are counted and skipped, never replaced lossily.
- Commit timestamps are displayed but never used for ordering, because they are not monotonic.
- Git LFS pointer files count as small text files in churn. Parsing skips them.

## Import graphs

- Edges show imports in source, not what runs. A dynamic import may never load.
- Only TS/TSX is analyzed. Files in other languages are counted, not parsed.
- `tsconfig` support reads `paths` and `baseUrl` from the root `tsconfig.json` only. `extends` is not followed (reported in coverage), and a monorepo package's own `tsconfig.json` is ignored, so its aliases show as unresolved.
- Package imports are omitted, and workspace packages imported by name count as external.
- `require()`, non-literal `import()`, and `import("./x").T` type expressions are not in the graph. The first two are counted.
- `.d.ts` files are excluded by default, so an import resolving only to one is reported as not found.
- Generated files that are not committed, such as `contentlayer/generated`, are unresolved.
- A file with syntax errors is parsed as far as possible and marked, so its imports may be incomplete.
- A repository over `limits.max_graph_nodes` or `max_graph_edges` keeps its history analysis but gets no graph.

## Change impact

- Scores are a ranking heuristic, not probabilities. One weight was tuned on development repositories, and [evaluation.md](evaluation.md) measures the result on others.
- Imports barely affect the score, so a critical import with no shared history can rank below files that merely change together.
- Files with no link to the change can appear, marked "Often changes".
- Co-change is correlation. Files can change together for unrelated reasons, and one merge can bundle unrelated work.
- Co-change covers TS/TSX files only, so changes to other files are not recommended.
- Conditional probability inflates files that change constantly. Base rate and lift are shown to expose this, but they do not affect the score.
- One or two shared commits count, with a caveat, so young files can rank on thin evidence.
- Each query reads up to 200 commits from Git, so it is not cached. Rankings are not stored; identical inputs reproduce them.
- A new file with no history and no importers gets only frequently changed files. That is missing evidence, not safety, and a note says so.

## Security

- The API is unauthenticated and relies on loopback binding plus `Host` and `Origin` checks. Other local processes and users on the same machine can reach it, and can open any Git repository readable by the user running the server.
- The `Host` and `Origin` checks defend browsers against remote pages. They do not defend against local malware.

## Architecture comparison

- Differences are in static imports. An added or removed import says what source declares, not what runs.
- Renames are similarity estimates. Between related commits on one first-parent line, CodeAtlas also follows renames step by step, which links a file that drifted over many commits. A file rewritten past the threshold within one commit stays one removal and one addition, because nothing in Git connects them. Across diverged branches, or more than 2,000 commits apart, only the two commits are compared, and a note says so.
- libgit2 scores similarity differently from `git`: 8 of 20 pairs on `shadcn-ui/taxonomy` differed. Direct renames are reported as libgit2 finds them.
- Two files that swap paths, or a new file created at a path that was renamed away, look like edits to the same paths.
- Cycles that share files are paired, largest overlap first. A cycle that splits pairs with one part and the others are new, and merged cycles work the reverse way. The pairing describes file membership, not intent.
- An import whose kind changes, such as to `import type`, shows as one removal and one addition.
- `uncertain` covers the importing file's own parse. An edge can also change with no source edit, because of `tsconfig.json` or a new target file, which notes describe only in total.
- Renames are limited to files in both graphs, so a move from `.js` to `.ts` is an added file.
- The timeline shows up to 40 evenly spaced commits of the latest analysis on the dashboard, so a change between two points can be hidden. Up to 200 are available from the API and CLI. Sampled counts come from each commit's own snapshot, and the first load builds any snapshots that are missing.

## Repository safety

- Repositories are opened as bare, so working tree files and `core.worktree` cannot affect results. A global Git config can still set attributes.
- A commit with more than 2 million paths, or nested past 512 levels, is refused.

## Backtesting

- It measures co-edit prediction, not bugs, causal impact or productivity. [evaluation.md](evaluation.md) lists its biases.
- The default warm-up of 100 eligible commits leaves small repositories with nothing to evaluate, and the result says so.
- A backtest evaluates up to 1,000 cases and cannot be cancelled yet.
- `make verify` covers one pinned commit in CI. The published results for the three repositories were manual runs.

## Planned scope limits

These apply once the features exist. [spec-review.md](spec-review.md) records open questions.

- TypeScript and TSX only. Unsupported languages are to be labelled, not silently dropped.
- Static imports only. Runtime `require`, non-literal dynamic imports, webpack aliases and workspace aliases are out of scope for v1.
- Impact scores will be a ranking heuristic, not a probability of failure.

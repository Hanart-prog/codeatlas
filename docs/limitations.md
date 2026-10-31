# Limitations

## Current state (Phase 3)

- No architecture comparison or backtesting exists, so ranking quality has not been measured.
- No benchmark has been run, so this project makes no performance claims.
- Three of the six dashboard sections are placeholders that say so.
- `codeatlas serve` serves the dashboard from `apps/web/dist`, so a binary alone serves only the API, and it must run from the repository root or be given `--web-dir`.
- Large graphs are cut to the most connected 500 files, with the true totals shown. Directory aggregation arrives in Phase 6.
- Only macOS arm64 has been tested locally. Linux is targeted by CI, which has not run yet. Windows is untested.
- Analysis jobs cannot be cancelled yet, and they live in memory, so job ids do not survive a restart. Results do.
- Each new tip commit reindexes the whole window. Incremental indexing is post-core.
- A process killed mid-analysis leaves its run marked `running`. It holds no history and does not block a later run.

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

- Scores are a ranking heuristic with hand-picked weights (0.6 static, 0.4 historical). They are not probabilities, and they have not been evaluated yet. Phase 5 measures them.
- With these weights, any direct importer (0.60) outranks a file with perfect co-change and no import (0.40).
- Co-change is correlation. Files can change together for unrelated reasons, and one merge can bundle unrelated work.
- Co-change covers TS/TSX files only, so changes to other files are not recommended.
- Conditional probability inflates files that change constantly. Base rate and lift are shown to expose this, but they do not affect the score.
- Pairs need 3 shared commits by default, so young files have little historical evidence, and a score can jump when a pair reaches the threshold.
- Each query reads up to 200 commits from Git, so it is not cached. Rankings are not stored; identical inputs reproduce them.
- A new file with no history and no importers gets no recommendations. That is missing evidence, not safety.

## Security

- The API is unauthenticated and relies on loopback binding plus `Host` and `Origin` checks. Other local processes and users on the same machine can reach it, and can open any Git repository readable by the user running the server.
- The `Host` and `Origin` checks defend browsers against remote pages. They do not defend against local malware.

## Planned scope limits

These apply once the features exist. [spec-review.md](spec-review.md) records open questions.

- TypeScript and TSX only. Unsupported languages are to be labelled, not silently dropped.
- Static imports only. Runtime `require`, non-literal dynamic imports, webpack aliases and workspace aliases are out of scope for v1.
- Backtests will measure co-edit prediction, not bugs, causal impact or productivity.
- Impact scores will be a ranking heuristic, not a probability of failure.

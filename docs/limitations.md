# Limitations

## Current state (Phase 1)

- No import parsing, dependency graph, co-change, impact ranking, comparison or backtesting exists.
- `parser_version` is 0 because no parser exists yet.
- No benchmark has been run, so this project makes no performance claims.
- Five of the six dashboard sections are placeholders that say so.
- The production frontend builds, but `codeatlas serve` does not serve it yet. It runs through Vite.
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
- Git LFS pointer files are counted as small text files. Parsing must skip them in Phase 2.

## Security

- The API is unauthenticated and relies on loopback binding plus `Host` and `Origin` checks. Other local processes and users on the same machine can reach it, and can open any Git repository readable by the user running the server.
- The `Host` and `Origin` checks defend browsers against remote pages. They do not defend against local malware.

## Planned scope limits

These apply once the features exist. [spec-review.md](spec-review.md) records open questions.

- TypeScript and TSX only. Unsupported languages are to be labelled, not silently dropped.
- Static imports only. Runtime `require`, non-literal dynamic imports, webpack aliases and workspace aliases are out of scope for v1.
- Backtests will measure co-edit prediction, not bugs, causal impact or productivity.
- Impact scores will be a ranking heuristic, not a probability of failure.

# 15. Cache commit diffs, and rebuild everything above them

**Status:** Accepted

Section 16 of the specification asks for incremental indexing. Patching a stored run forward looks cheapest, but file identities depend on the whole walked window: moving the window's start or following a rename changes them. Patching them correctly means reproducing the replay, with a second code path that can drift from the first.

**Decision:** cache one thing, a commit's diff against its first parent, in `commit_diffs`. That value depends only on the two commits and the settings. The key holds `ALGORITHM_VERSION`, the analysis settings hash and whether the clone is shallow. A new tip reads only its new commits from Git, and identities, churn and storage are then rebuilt by the same code a first run uses.

Impact, comparison, the Explorer and backtests read through the same cache, so they stop rediffing up to 200 commits per query.

**Consequences:**

- There is one code path, and tests assert that cached and fresh results are identical for overview, graph, impact, comparison, timeline and backtests. `benchmarks/run.py` also fails if they differ on any measured repository.
- A new run still writes a full copy of its window. This costs disk, not correctness. Old runs are kept, because stored backtests belong to them.
- Changing how diffs are computed must bump `ALGORITHM_VERSION`, or stale diffs would be reused.
- A cancelled analysis keeps the diffs it read, so running it again continues.

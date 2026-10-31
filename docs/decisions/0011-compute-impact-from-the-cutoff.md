# 11. Compute impact from the cutoff commit

**Status:** Accepted

The specification stores co-change per cutoff in `cochange_edges` and rankings in `impact_runs` and `impact_results`. Temporal correctness is the feature's central requirement, and filtering stored data by position leaves a way for later commits to leak in.

**Decision:** each query walks Git from the cutoff commit and reads that commit's import snapshot. Later commits are never read, so look-ahead is impossible by construction. Nothing is stored, because identical inputs reproduce identical rankings.

**Consequence:** a query costs up to `history_window` diffs (0.23s on one 592-commit repository in one run, not a benchmark). Phase 5 will need an incremental form of the same logic for hundreds of cutoffs. Tests must keep proving no leakage: they assert that later commits leave a historical ranking byte-identical.

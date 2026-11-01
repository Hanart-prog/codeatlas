# 13. Backtest through the Change Impact ranking

**Status:** Accepted

A backtest is only useful if it measures what users get. A separate implementation could be faster, and could drift from the real ranking without anyone noticing.

**Decision:** Change Impact and the backtest call one `rank` function. Each case passes it a history slice ending at the parent and the parent's snapshot. Baselines reuse it with different weights where they can. Git is read once for the whole chain, and identities are replayed per case. Results are stored in `backtest_runs`, keyed by the analysis run and every parameter that affects them.

**Consequence:** the cost grows with cases times window. It took 3 to 4 seconds for up to 179 cases, in one manual run that is not a benchmark, and a repeat returns the stored result. Tests assert that each case equals a real Change Impact query, so the two cannot drift apart. The specification's `params_json` and `metrics_json` columns are kept, and the unique key adds what ADR 11 chose not to store for single queries, because a backtest is too expensive to repeat on every view.

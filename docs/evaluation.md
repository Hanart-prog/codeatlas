# Evaluation

CodeAtlas backtests its own recommendations on later commits, with baselines. This page gives the protocol and the measured results, including their limits.

**What this measures:** co-edit prediction, meaning whether files changed together in a later commit. It does not measure bugs, real causal impact or developer productivity, and a held-out file may have changed for an unrelated reason.

## Result

**On six repositories held out from development, Change Impact hit a changed file in its top 5 for 46% of commits, against 20% for the most-changed baseline and 5% for random.** Its interval sits above the baseline's on five of the six; on `excalidraw` the intervals overlap. Scores are macro-averaged across repositories.

The method was fixed in advance. The six repositories, the primary metrics and the success rule (mean at or above most-changed) were written down before any ranking work, the formula was chosen on five other repositories only, and it was frozen before its one held-out run. After the engine fixes that followed, the same formula was run again to confirm the figures, and they did not change.

### Held-out repositories

Hit rate at 5 files, with bootstrap 95% intervals, warm-up 100, on the last 800 commits of each repository:

| Repository | Cases | Change Impact | Co-change only | Imports only | Most changed | Random |
|---|---:|---:|---:|---:|---:|---:|
| `trpc/trpc` | 139 | 38.8% (31 to 47) | 31.7% | 18.0% | 11.5% (6 to 17) | 3.6% |
| `vuejs/core` | 200 | 45.0% (38 to 52) | 38.5% | 2.0% | 18.0% (12 to 23) | 5.0% |
| `excalidraw/excalidraw` | 200 | 56.0% (48 to 63) | 43.5% | 16.0% | 45.5% (39 to 52) | 8.5% |
| `typeorm/typeorm` | 200 | 42.5% (35 to 49) | 34.5% | 24.0% | 26.0% (20 to 33) | 2.5% |
| `nestjs/nest` | 200 | 63.0% (56 to 69) | 53.5% | 47.0% | 12.5% (8 to 17) | 3.0% |
| `TanStack/query` | 200 | 30.5% (24 to 37) | 25.5% | 14.5% | 7.5% (4 to 11) | 4.5% |
| **Mean** | | **46.0%** | 37.9% | 20.2% | 20.2% | 4.5% |

Recall at 10 files averaged 36.5%, against 13.1% for most changed and 2.7% for random.

The previous formula, run on the same repositories for comparison, averaged 24.5% on the five it could analyze, against 42.6% for the current one on those five. It could not analyze `nest` at all, because of a storage bug this work exposed and fixed.

### Development repositories

These chose the formula, so they flatter it and are not evidence of how it generalizes:

| Repository | Cases | Change Impact | Most changed | Random |
|---|---:|---:|---:|---:|
| `sindresorhus/ky` | 179 | 77.1% (70 to 83) | 74.9% (68 to 81) | 27.4% |
| `colinhacks/zod` | 129 | 79.8% (73 to 87) | 55.0% (47 to 64) | 7.0% |
| `honojs/hono` | 200 | 59.0% (52 to 66) | 11.5% (7 to 16) | 2.5% |
| `pmndrs/jotai` | 128 | 71.1% (63 to 80) | 54.7% (45 to 62) | 14.1% |

`shadcn-ui/taxonomy` has too little history for warm-up 100 and was not used.

### What changed

The first formula scored `0.6 * (1 / distance) + 0.4 * P(candidate | selected)`, and ignored any pair with fewer than 3 shared commits. It lost to most-changed files. The evidence pointed to three causes:

- **Most files are young.** The median input had 2 to 5 commits on `zod`, `hono` and `jotai`, so the support rule discarded nearly all its history. Keeping that rule cost more than anything else tested: hit rate at 5 across the development set was 59% with it and 70% without.
- **Imports were overweighted.** With 0.6 on imports, any importer outranked strong history. Imports alone are a weak predictor, and work best as a tiebreak.
- **Empty answers.** Many inputs had nothing to rank, so slots went unused.

The current score is:

```text
score = (shared + candidate commits / eligible commits) / (selected commits + 1) + 0.05 / distance
```

The first term is the chance a file changes with the selected one, shrunk toward how often it changes at all. One shared commit counts a little, and a file with no shared commit falls back to its own activity, marked "Often changes" in results so that it is not mistaken for a link. Recency weighting was tested and added nothing, so it was left out.

The baseline is still a useful bar: it gives the same list for every input, so beating it shows the ranking depends on the file being changed. Part of Change Impact's score is that same activity signal, which helps it stay near the baseline when evidence is thin.

**One weight is tuned, and only on development data:** the 0.05 import weight, where results barely moved between 0.02 and 0.2.

## Protocol

1. Walk the first-parent chain of the latest analysis.
2. Set aside commits with no eligible file, and bulk commits over 50 files. Eligible means TS or TSX and not excluded, the definition co-change uses.
3. Use the first eligible commits as warm-up only (default 100).
4. Each later eligible commit is a candidate case. Skip one with fewer than 2 eligible files, or with fewer than 2 that existed at its parent.
5. The input is the smallest path among files that existed at the parent. The other files are held-out targets, at their parent paths. New files are left out and counted, because they could never be recommended.
6. Rank from the parent only: its import snapshot and the window of up to 200 commits ending at it.
7. Evenly sample down to `max_cases` (default 200), and compare top-k rankings with the targets.

Cases are macro-averaged, so a large commit counts the same as a small one.

| Metric | Definition |
|---|---|
| `precision@k` | `hits / k`, even if fewer than `k` files were ranked. Mean candidate counts are shown beside it |
| `recall@k` | `hits / held-out targets` |
| `hit_rate@k` | Share of cases with at least one hit |
| `coverage` | `evaluable cases / candidate cases` |

Intervals are percentile bootstraps over cases, with 1,000 resamples from a fixed seed. Overlapping intervals are reported as "cannot separate", which is cautious: two overlapping intervals can still hide a real difference.

| Ranker | What it does |
|---|---|
| Change Impact | The real ranking, through the same function the Change Impact page uses |
| Co-change only | The same score with no import weight, among files sharing a commit with the input |
| Imports only | `1 / distance`, among files that import the input |
| Most changed | Files changed most often in the window, ties by path |
| Random | A seeded shuffle of those same files |

## Preventing look-ahead

A case receives a slice of history that ends at its parent, and the parent's snapshot. Later commits never reach the ranking code, rather than being filtered out of it.

- **Same ranking:** tests check that every case equals a real Change Impact query at its parent, for three window sizes.
- **Later commits:** a test adds commits written to change the answers, and earlier cases stay identical.
- **Own commit:** a test shows co-change evidence appears only once support is reached before the case.
- **Planted leaks:** four deliberate leaks were each caught by these tests.
- **Real clones:** `scripts/verify_backtest.py` reruns the backtest on a clone cut off partway through history. On the final formula, all 443 shared cases across `ky`, `zod` and `hono` were identical.

## Known biases

- The smallest path favours some directories, which the seeded input result exposes.
- Merge commits count once and can bundle unrelated work.
- The same commits feed the history of later cases, so cases are not independent and intervals may be too narrow.
- The baseline pool holds only files changed in the window, so random and most-changed never rank a file untouched for 200 commits.
- A held-out file that changed for an unrelated reason counts as a target, so no ranker can reach 100%.
- The clones stop at 800 commits, so each result covers recent history only.
- Six repositories, picked as well-known TypeScript projects, are not a random sample. Large, active libraries may behave differently from application code.
- Renames depend on similarity scoring, and files renamed by an evaluated commit are scored at their parent path.

## Reproducing

```bash
codeatlas analyze /path/to/repo --history 5000
codeatlas backtest /path/to/repo --warmup 100 --top-k 5,10 --format json
make verify REPO=/path/to/clone WARMUP=100
```

Identical inputs give byte-identical results, and every case is in the JSON output so that it can be audited.

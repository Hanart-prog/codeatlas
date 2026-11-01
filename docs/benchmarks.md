# Benchmarks

Measured by `make bench` on one machine: Apple M3 Pro, 12 logical CPUs, 36 GiB of memory, Darwin 23.2.0 arm64, with `codeatlas 0.1.0` built in release mode. Each task ran 5 times, and figures are medians. [results.json](../benchmarks/results.json) has every range, and the dashboard's Benchmarks page shows the same file.

## Indexing

Analysis reads the newest 500 first-parent commits by default, so `zod` and `nest` are measured on 500 commits, not their full history. The commit column shows the whole history.

| Repository | Commits in history | TS files | First analysis | Same commit again | 20 new commits, cached | Same tip, no cache | Peak memory |
|---|---:|---:|---:|---:|---:|---:|---:|
| generated demo | 180 | 24 | 0.092s | 0.005s | 0.028s | 0.088s | 10 MiB |
| generated 5,000 files, 300 commits | 301 | 5,000 | 1.182s | 0.010s | 0.274s | 1.158s | 38 MiB |
| sindresorhus/ky | 592 | 98 | 0.429s | 0.006s | 0.172s | 0.425s | 53 MiB |
| colinhacks/zod | 2,305 | 515 | 0.971s | 0.007s | 0.148s | 0.974s | 101 MiB |
| nestjs/nest | 8,891 | 2,269 | 1.317s | 0.007s | 0.139s | 1.269s | 66 MiB |

- **First analysis** is a cold run on an empty database.
- **Same commit again** reuses the completed run.
- **20 new commits, cached** analyzes the tip with an analysis 20 commits older already in the database.
- **Same tip, no cache** is a full reindex of that same tip, on an empty database.

The script compares the overview from the cached and the full run on every repetition, and stops if they differ. They never did. The test suite extends that check to graphs, rankings, comparisons, timelines and backtests on fixtures.

The cached run was 2.5 to 9.1 times faster here. It is not free, because every run still rewrites its whole window to the database and rebuilds file identities.

## Queries

After a first analysis:

| Repository | Read the graph | Rank impact | Backtest, up to 50 cases | Backtest peak memory | Database size |
|---|---:|---:|---:|---:|---:|
| generated demo | 0.005s | 0.010s | 0.14s | 12 MiB | 0.5 MiB |
| generated 5,000 files, 300 commits | 0.024s | 0.125s | 10.09s | 128 MiB | 10.2 MiB |
| sindresorhus/ky | 0.006s | 0.008s | 1.44s | 46 MiB | 1.4 MiB |
| colinhacks/zod | 0.008s | 0.012s | 3.15s | 74 MiB | 2.2 MiB |
| nestjs/nest | 0.016s | 0.024s | 3.77s | 86 MiB | 5.6 MiB |

The CLI figures include process start and opening the database. The backtest used a warm-up of 20. It builds a graph for each case, so it grows with files: the 5,000-file generated repository is the slowest result here.

## What these do not show

- One machine, with results that will differ elsewhere. No other tool was measured, so there is no comparison.
- The 5,000-file repository is generated, and its shape is arbitrary.
- Peak memory is the process's peak resident size, from `wait4`. A cold run's disk cache state was not controlled, so the first repetition may have read Git objects more slowly.
- Browser rendering is not timed here. The Playwright suite checks that a 3,000-file repository opens on a directory map and stays usable.

## Repositories

Public repositories are cloned to a temporary directory at these commits and deleted afterwards. Their licenses were checked on GitHub.

- `generated demo` at `73597ad633fb81f312b0f5b54c02d57d734c1bff` (generated)
- `generated 5,000 files, 300 commits` at `5f7e61c99cef395fe0841ef81454d80911b419a6` (generated)
- `sindresorhus/ky` at `3541888878c4d303a8bc2ad61815f4e048f55a5d` (MIT)
- `colinhacks/zod` at `2554b70c3d4a80273381e61d527d820720626440` (MIT)
- `nestjs/nest` at `8d9dd9d174921cc03882d1b39e91fb7bb1b1ca5a` (MIT)

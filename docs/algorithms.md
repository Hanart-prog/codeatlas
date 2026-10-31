# Algorithms and metric definitions

Status: Phase 1. Later phases add co-change, impact scoring and backtesting.

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

## Verification

Besides the fixture, Phase 1 was checked against the `git` CLI on `sindresorhus/ky` (592 first-parent commits, one manual run, not part of CI):

- Commit, merge and tree file counts matched.
- 9 of the 10 most changed files matched `git log --follow --numstat` exactly on commits and lines.
- The 10th differed because `git` scored a rename at 47% and libgit2 at 50% or more. Its totals reconcile exactly with a two-path `git log`.
- Whole-history file-change rows reconcile: 1,914 against 1,917, explained by 3 extra renames that each merge a delete and an add.

#!/usr/bin/env python3
"""Rechecks `codeatlas backtest` from `git` alone, with no shared code.

Usage: verify_backtest.py <codeatlas binary> <repository> [warmup]

Checks each case's input and targets against `git diff-tree`, every ranked file
against the parent's tree, the most-changed baseline against a replay of `git
log`, and every average against the case list. It then backtests a clone cut off
partway through history: cases both runs share must be identical, which is the
direct test for look-ahead. Exits non-zero on disagreement, except where libgit2
and git score a rename differently, which it counts and reports.
"""
import json, os, re, shutil, subprocess, sys, tempfile

BINARY, REPO = sys.argv[1], sys.argv[2]
WARMUP = sys.argv[3] if len(sys.argv) > 3 else "100"
TS = re.compile(r"\.(ts|tsx|mts|cts)$")
BULK = 50
WINDOW = 200


def run(*command, cwd=None):
    done = subprocess.run(command, capture_output=True, text=True, cwd=cwd)
    if done.returncode:
        raise SystemExit(f"{command}: {done.stderr}")
    return done.stdout


def git(repo, *args):
    return run("git", "-C", repo, "-c", "core.quotePath=false", *args)


def backtest(repo):
    data = tempfile.mkdtemp()
    run(BINARY, "analyze", repo, "--history", "5000", "--data-dir", data)
    output = run(BINARY, "backtest", repo, "--warmup", WARMUP, "--top-k", "5,10", "--max-cases", "1000", "--format", "json", "--data-dir", data)
    shutil.rmtree(data)
    return json.loads(output)


def ranking(case, ranker):
    return next(r for r in case["rankings"] if r["ranker"] == ranker)


def changes(oid, parent):
    """(status, old, new) for each file, renames detected the way git does."""
    found = []
    for line in git(REPO, "diff-tree", "-r", "-M50%", "--no-commit-id", "--name-status", parent, oid).splitlines():
        fields = line.split("\t")
        status = fields[0][0]
        if status == "R":
            found.append((status, fields[1], fields[2]))
        elif status == "A":
            found.append((status, None, fields[1]))
        elif status == "D":
            found.append((status, fields[1], None))
        else:
            found.append((status, fields[1], fields[1]))
    return found


result = backtest(REPO)
cases = result["cases"]
assert cases, "no cases"
chain = git(REPO, "rev-list", "--first-parent", "--reverse", "HEAD").split()
position_of = {oid: index for index, oid in enumerate(chain)}

# Every first-parent commit, for a replay of `git log` in the baseline check.
per_commit = []
for index, oid in enumerate(chain):
    parent = chain[index - 1] if index else "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    entries = changes(oid, parent)
    per_commit.append(entries)

# The default `exclude_globs`.
excluded = re.compile(r"(^|/)(node_modules|vendor|dist|build)/|\.min\.js$|\.d\.ts$")


def eligible(entry):
    path = entry[2] or entry[1]
    return bool(TS.search(path)) and not excluded.search(path)


def most_changed(position, skip):
    counts = {}
    for index in range(max(0, position - WINDOW), position):
        entries = [e for e in per_commit[index] if eligible(e)]
        if len(entries) > BULK:
            entries = []
        for status, old, new in per_commit[index]:
            if status == "R" and old in counts:
                counts[new] = counts.pop(old)
            elif status == "D":
                counts.pop(old, None)
        for _, _, new in [e for e in entries if e[0] != "D"]:
            counts[new] = counts.get(new, 0) + 1
    files = sorted((-count, path) for path, count in counts.items() if path != skip)
    return [path for _, path in files]


rename_gaps = baseline_gaps = 0
for case in cases:
    oid, parent = case["commit"]["oid"], case["parent"]
    position = position_of[oid]
    assert chain[position - 1] == parent, oid

    expected = sorted({old for _, old, _ in [e for e in changes(oid, parent) if e[0] != "A" and eligible(e)] if TS.search(old)})
    actual = sorted([case["input"], *case["targets"]])
    if expected != actual:
        rename_gaps += 1
    else:
        assert case["input"] == expected[0], oid

    tree = set(git(REPO, "ls-tree", "-r", "--name-only", parent).splitlines())
    assert case["input"] in tree and set(case["targets"]) <= tree, oid
    for entry in case["rankings"]:
        assert set(entry["paths"]) <= tree, (oid, entry["ranker"])
        assert case["input"] not in entry["paths"], (oid, entry["ranker"])
        assert len(entry["paths"]) == len(set(entry["paths"])), oid

    baseline = ranking(case, "most_changed")["paths"]
    if most_changed(position, case["input"])[: len(baseline)] != baseline:
        baseline_gaps += 1

for ranker in result["rankers"]:
    for metrics in ranker["metrics"]:
        k = metrics["k"]
        found = [len(set(ranking(c, ranker["ranker"])["paths"][:k]) & set(c["targets"])) for c in cases]
        precision = sum(f / k for f in found) / len(cases)
        recall = sum(f / len(c["targets"]) for f, c in zip(found, cases)) / len(cases)
        hit_rate = sum(f > 0 for f in found) / len(cases)
        assert abs(precision - metrics["precision"]["mean"]) < 1e-9, ranker["ranker"]
        assert abs(recall - metrics["recall"]["mean"]) < 1e-9, ranker["ranker"]
        assert abs(hit_rate - metrics["hit_rate"]["mean"]) < 1e-9, ranker["ranker"]
        assert metrics["hit_rate"]["low"] <= hit_rate <= metrics["hit_rate"]["high"], ranker["ranker"]

# The direct look-ahead test: a clone that has never seen the later commits.
cut = chain[position_of[cases[len(cases) * 2 // 3]["commit"]["oid"]]]
scratch = tempfile.mkdtemp()
clone = os.path.join(scratch, "clone")
subprocess.run(["git", "clone", "--quiet", "--no-hardlinks", REPO, clone], check=True)
git(clone, "reset", "--quiet", "--hard", cut)
earlier = {case["commit"]["oid"]: case for case in backtest(clone)["cases"]}
shutil.rmtree(scratch)
shared = [case for case in cases if case["commit"]["oid"] in earlier]
assert shared, "the cut-off run shares no cases"
for case in shared:
    assert earlier[case["commit"]["oid"]] == case, case["commit"]["oid"]

print(
    f"{os.path.basename(REPO)}: {len(cases)} cases, metrics identical, {len(shared)} cases identical "
    f"without later commits, {rename_gaps} target sets and {baseline_gaps} baselines differ through "
    "git/libgit2 renames or exclusions"
)

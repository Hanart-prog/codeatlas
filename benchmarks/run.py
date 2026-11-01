#!/usr/bin/env python3
"""Measures CodeAtlas on a generated fixture and on public repositories.

Every figure comes from running the release binary here. Nothing is estimated, and
third-party source is cloned at a pinned commit to a temporary directory, never
kept in this repository.

Scenarios, each with a fresh data directory unless noted:
  cold         first analysis, including the import graph
  warm         the same commit again, served from the completed run
  new_commits  a tip N commits later, with the older tip's caches in place
  full_reindex that same tip with no caches, to compare against new_commits
  graph, impact, backtest   queries after an analysis

new_commits and full_reindex must give identical overviews; the script fails
otherwise, because a faster wrong answer is not a speed-up.

Usage: benchmarks/run.py [--reps N] [--output FILE] [--skip-public]
"""

import argparse
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target/release/codeatlas"
FIXTURE_TOOL = ROOT / "target/release/generate-fixture"
NEW_COMMITS = 20

# Permissively licensed. Commits are pinned so that results can be repeated.
PUBLIC_REPOSITORIES = [
    ("sindresorhus/ky", "MIT", "3541888878c4d303a8bc2ad61815f4e048f55a5d"),
    ("colinhacks/zod", "MIT", "2554b70c3d4a80273381e61d527d820720626440"),
    ("nestjs/nest", "MIT", "8d9dd9d174921cc03882d1b39e91fb7bb1b1ca5a"),
]


def timed(command):
    """Runs a command, returning (seconds, peak RSS in MiB, stdout)."""
    started = time.perf_counter()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    stdout = process.stdout.read()
    stderr = process.stderr.read()
    _, status, usage = os.wait4(process.pid, 0)
    elapsed = time.perf_counter() - started
    process.returncode = os.waitstatus_to_exitcode(status)
    if process.returncode != 0:
        sys.exit(f"failed: {' '.join(map(str, command))}\n{stderr}")
    divisor = 1024 * 1024 if sys.platform == "darwin" else 1024
    return elapsed, usage.ru_maxrss / divisor, stdout


def git(repository, *args):
    return subprocess.run(
        ["git", "-C", str(repository), *args], check=True, capture_output=True, text=True
    ).stdout.strip()


def describe(repository):
    head = git(repository, "rev-parse", "HEAD")
    files = git(repository, "ls-tree", "-r", "-l", "HEAD").splitlines()
    sources = [line.split(None, 4) for line in files if line.endswith((".ts", ".tsx", ".mts", ".cts"))]
    return {
        "commit": head,
        "first_parent_commits": int(git(repository, "rev-list", "--first-parent", "--count", "HEAD")),
        "ts_files": len(sources),
        "ts_bytes": sum(int(entry[3]) for entry in sources if entry[3].isdigit()),
    }


def stable_overview(binary_args):
    """An overview with the fields that differ between runs removed."""
    _, _, stdout = timed(binary_args)
    overview = json.loads(stdout)
    overview["run"].pop("id")
    overview["run"].pop("completed_at")
    overview["repository"].pop("id")
    overview["repository"].pop("analyzed_at")
    return overview


def summarize(samples):
    seconds = [sample[0] for sample in samples]
    memory = [sample[1] for sample in samples]
    return {
        "median_seconds": round(statistics.median(seconds), 3),
        "min_seconds": round(min(seconds), 3),
        "max_seconds": round(max(seconds), 3),
        "peak_rss_mib": round(max(memory), 1),
        "repetitions": len(samples),
    }


def fresh(work):
    return Path(tempfile.mkdtemp(dir=work))


def measure(repository, work, reps, query_file):
    commits = int(git(repository, "rev-list", "--first-parent", "--count", "HEAD"))
    older = f"HEAD~{min(NEW_COMMITS, commits - 2)}"
    analyze = lambda data, ref="HEAD": [BINARY, "analyze", repository, "--ref", ref, "--format", "json", "--data-dir", data]
    results = {key: [] for key in ("cold", "warm", "new_commits", "full_reindex", "graph", "impact", "backtest")}

    for _ in range(reps):
        data = fresh(work)
        results["cold"].append(timed(analyze(data)))
        results["warm"].append(timed(analyze(data)))
        results["graph"].append(timed([BINARY, "graph", repository, "--format", "json", "--data-dir", data]))
        if query_file:
            results["impact"].append(
                timed([BINARY, "impact", repository, "--file", query_file, "--format", "json", "--data-dir", data])
            )
        results["backtest"].append(
            timed([BINARY, "backtest", repository, "--warmup", "20", "--max-cases", "50", "--format", "json", "--data-dir", data])
        )
        shutil.rmtree(data)

        data = fresh(work)
        timed(analyze(data, older))
        results["new_commits"].append(timed(analyze(data)))
        incremental = stable_overview([BINARY, "overview", repository, "--format", "json", "--data-dir", data])
        shutil.rmtree(data)

        data = fresh(work)
        results["full_reindex"].append(timed(analyze(data)))
        full = stable_overview([BINARY, "overview", repository, "--format", "json", "--data-dir", data])
        database_mib = (Path(data) / "codeatlas.sqlite").stat().st_size / (1024 * 1024)
        shutil.rmtree(data)

        if incremental != full:
            sys.exit(f"incremental and full results differ on {repository}")

    summary = {key: summarize(samples) for key, samples in results.items() if samples}
    summary["database_mib"] = round(database_mib, 1)
    summary["new_commit_count"] = min(NEW_COMMITS, commits - 2)
    return summary


def most_changed_ts_file(repository, work):
    data = fresh(work)
    timed([BINARY, "analyze", repository, "--data-dir", data])
    _, _, stdout = timed([BINARY, "overview", repository, "--format", "json", "--data-dir", data])
    shutil.rmtree(data)
    for file in json.loads(stdout)["top_churn"]:
        if file["exists_at_tip"] and file["path"].endswith((".ts", ".tsx")):
            return file["path"]
    return None


def machine():
    if sys.platform == "darwin":
        cpu = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip()
        memory = int(subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True).stdout)
    else:
        cpu = platform.processor() or platform.machine()
        memory = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    return {
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "memory_gib": round(memory / 1024**3),
        "os": f"{platform.system()} {platform.release()} {platform.machine()}",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--reps", type=int, default=5)
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("results.json"))
    parser.add_argument("--skip-public", action="store_true")
    args = parser.parse_args()
    if args.reps < 3:
        parser.error("use at least 3 repetitions")

    report = {
        "tool": timed([BINARY, "--version"])[2].strip(),
        "machine": machine(),
        "repetitions": args.reps,
        "subjects": [],
    }

    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        subjects = []
        for name, flags in [("generated demo", ["--demo"]), ("generated 5,000 files, 300 commits", ["--scale", "5000", "300"])]:
            repository = work / name.replace(" ", "-").replace(",", "")
            repository.mkdir()
            subprocess.run([FIXTURE_TOOL, repository, *flags], check=True, capture_output=True)
            subjects.append((name, "generated", repository))
        if not args.skip_public:
            for name, license_name, commit in PUBLIC_REPOSITORIES:
                repository = work / name.replace("/", "-")
                subprocess.run(["git", "clone", "--quiet", f"https://github.com/{name}", repository], check=True)
                git(repository, "checkout", "--quiet", commit)
                subjects.append((name, license_name, repository))

        for name, license_name, repository in subjects:
            print(f"measuring {name}", file=sys.stderr)
            query_file = most_changed_ts_file(repository, work)
            report["subjects"].append(
                {
                    "name": name,
                    "license": license_name,
                    **describe(repository),
                    "impact_file": query_file,
                    "results": measure(repository, work, args.reps, query_file),
                }
            )

    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()

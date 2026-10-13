#!/usr/bin/env -S uv run
"""Reproduce Git graph and cache benchmarks using disposable synthetic repositories.

Example:
  uv run scripts/benchmark-performance.py --candidate target/release/git-ls \
    --baseline /tmp/old-git-ls --directory /tmp/git-ls-bench --repeats 9

Results retain every sample. Repositories and Git configuration are isolated;
normal filesystem caches remain warm. Timing order is reproducibly shuffled.
"""

import argparse
import hashlib
import json
import os
import pathlib
import platform
import random
import statistics
import subprocess
import tempfile
import time


def git(repo: pathlib.Path, *args: str, data: str | None = None) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        input=data,
        text=True,
        capture_output=True,
        env=ENV,
        check=True,
    ).stdout.strip()


def fixture(directory: pathlib.Path, depth: int, branches: int, mode: str) -> pathlib.Path:
    repo = directory / f"n{depth}-b{branches}-{mode}"
    if repo.exists():
        return repo
    repo.mkdir(parents=True)
    git(repo, "-c", "init.templateDir=", "init", "-q", "-b", "main")
    git(repo, "config", "gc.auto", "0")
    records = []
    for index in range(1, depth + 1):
        parent = f"from :{index - 1}\n" if index > 1 else ""
        records.append(
            f"commit refs/heads/main\nmark :{index}\n"
            f"committer Benchmark <benchmark@example.invalid> {1700000000 + index} +0000\n"
            f"data 2\nm\n{parent}\n"
        )
    for index in range(branches):
        base = depth if mode == "tip" else depth - ((index + 1) * (depth // 2) // branches)
        records.append(
            f"commit refs/heads/topic-{index:03}\nmark :{depth + index + 1}\n"
            f"committer Benchmark <benchmark@example.invalid> {1700000001 + depth + index} +0000\n"
            f"data 2\nb\nfrom :{base}\n\n"
        )
    git(repo, "fast-import", "--quiet", data="".join(records))
    return repo


def measure(
    binary: pathlib.Path, repo: pathlib.Path, flags: list[str]
) -> tuple[dict[str, float | None], str]:
    command = [str(binary), "--color", "never", "--layout", "inline", "--verbosity", "0", *flags]
    with tempfile.TemporaryFile() as diagnostics:
        started = time.perf_counter()
        child = subprocess.Popen(
            command, cwd=repo, env=ENV, stdout=subprocess.DEVNULL, stderr=diagnostics
        )
        if hasattr(os, "wait4"):
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            rss = usage.ru_maxrss * (1 if platform.system() == "Darwin" else 1024)
            cpu = (usage.ru_utime + usage.ru_stime) * 1000
        else:
            child.wait()
            rss, cpu = None, None
        elapsed = (time.perf_counter() - started) * 1000
        diagnostics.seek(0)
        debug = diagnostics.read().decode(errors="replace")
        if child.returncode:
            raise RuntimeError(f"{command}: {debug}")
    return {"elapsed_ms": elapsed, "peak_rss_bytes": rss, "cpu_ms": cpu}, debug


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", type=pathlib.Path)
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--repeats", type=int, default=9)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    args.directory.mkdir(parents=True, exist_ok=True)
    candidate = args.candidate.resolve()
    modes = {
        "uncached": (candidate, ["--no-cache"]),
        "refresh": (candidate, ["--refresh-cache", "--no-cache-maintenance"]),
        "warm": (candidate, ["--no-cache-maintenance"]),
        "debug_uncached": (candidate, ["--no-cache", "--debug"]),
        "debug_warm": (candidate, ["--no-cache-maintenance", "--debug"]),
    }
    if args.baseline:
        baseline = args.baseline.resolve()
        help_text = subprocess.check_output([str(baseline), "--help"], text=True)
        modes["baseline"] = (baseline, ["--no-cache"] if "--no-cache" in help_text else [])
    report = {"platform": platform.platform(), "repeats": args.repeats, "cases": []}
    report["versions"] = {
        name: subprocess.check_output([str(binary), "--version"], text=True).strip()
        for name, (binary, _) in modes.items()
    }
    report["binary_sha256"] = {
        name: hashlib.sha256(binary.read_bytes()).hexdigest() for name, (binary, _) in modes.items()
    }
    for depth, branches, mode in [
        (10000, 1, "tip"),
        (10000, 32, "distributed"),
        (50000, 8, "distributed"),
        (10000, 128, "tip"),
    ]:
        repo = fixture(args.directory, depth, branches, mode)
        samples = {name: [] for name in modes}
        for binary, flags in modes.values():
            measure(binary, repo, flags)
        for repetition in range(args.repeats):
            order = list(modes)
            random.Random(repetition).shuffle(order)
            for name in order:
                binary, flags = modes[name]
                sample, _ = measure(binary, repo, flags)
                samples[name].append(sample)
        result = {
            "fixture": repo.name,
            "modes": {
                name: {
                    "median_ms": statistics.median(sample["elapsed_ms"] for sample in values),
                    "samples": values,
                }
                for name, values in samples.items()
            },
        }
        _, debug = measure(candidate, repo, ["--no-cache", "--debug"])
        (args.directory / f"{repo.name}.debug.txt").write_text(debug)
        report["cases"].append(result)
        (args.directory / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(
            repo.name,
            {name: round(value["median_ms"], 3) for name, value in result["modes"].items()},
            flush=True,
        )


ENV = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
ENV.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1", LC_ALL="C")
if __name__ == "__main__":
    main()

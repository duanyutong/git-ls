#!/usr/bin/env -S uv run
"""Measure debug output transport and cache-maintenance interference.

All cache mutations are confined to a disposable synthetic repository created
under --directory. A drained pseudo-terminal measures transport costs, not a
terminal emulator's rendering latency. Requires Unix wait4 and pseudo-terminals.
"""

import argparse
import errno
import json
import os
import pathlib
import platform
import pty
import random
import runpy
import statistics
import subprocess
import threading
import time
from typing import Any


def wait_usage(child: subprocess.Popen[bytes]) -> dict[str, float]:
    _, status, usage = os.wait4(child.pid, 0)
    child.returncode = os.waitstatus_to_exitcode(status)
    if child.returncode:
        raise RuntimeError(f"child failed: {child.args}")
    return {
        "cpu_ms": 1000 * (usage.ru_utime + usage.ru_stime),
        "peak_rss_bytes": usage.ru_maxrss * (1 if platform.system() == "Darwin" else 1024),
    }


def foreground(
    binary: pathlib.Path, repo: pathlib.Path, env: dict[str, str], terminal: bool = False
) -> dict[str, Any]:
    command = [str(binary), "--debug", "--color", "never", "--no-cache-maintenance"]
    started = time.perf_counter()
    render = None
    if terminal:
        master, slave = pty.openpty()
        child = subprocess.Popen(
            command, cwd=repo, env=env, stdout=subprocess.DEVNULL, stderr=slave
        )
        os.close(slave)
        try:
            while True:
                try:
                    if not os.read(master, 65536):
                        break
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
        finally:
            os.close(master)
    else:
        child = subprocess.Popen(
            command, cwd=repo, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE
        )
        assert child.stderr is not None
        for line in child.stderr:
            if b"stage=write_output " in line and b"status=ok" in line:
                render = (time.perf_counter() - started) * 1000
        child.stderr.close()
    result: dict[str, Any] = dict(wait_usage(child))
    result.update(
        elapsed_ms=(time.perf_counter() - started) * 1000, render_record_received_ms=render
    )
    return result


def collect_worker(
    child: subprocess.Popen[bytes], result: dict[str, float], started: float
) -> None:
    """Collect usage for the explicitly supplied worker rather than loop state."""
    result.update(wait_usage(child))
    result["elapsed_ms"] = (time.perf_counter() - started) * 1000


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=pathlib.Path, required=True)
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--repeats", type=int, default=9)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    args.directory.mkdir(parents=True, exist_ok=True)
    library = runpy.run_path(str(pathlib.Path(__file__).with_name("benchmark-performance.py")))
    env = library["ENV"]
    binary = args.candidate.resolve()
    repo = library["fixture"](args.directory.resolve(), 10000, 32, "distributed")
    library["measure"](binary, repo, ["--refresh-cache", "--no-cache-maintenance"])
    root = repo / ".git/git-ls/cache/v1"
    # The fixture uses one stable reference snapshot. Preserve only its actual
    # snapshot when constructing the near-quota namespace for each sample.
    candidates = list(root.glob("snapshot-*.json"))
    retain = next(path for path in candidates if path.stat().st_size < 1024 * 1024)
    retained_bytes = retain.read_bytes()
    results = {name: [] for name in ["pipe_debug", "pty_debug", "maintenance_debug"]}
    for repetition in range(args.repeats):
        order = list(results)
        random.Random(repetition).shuffle(order)
        for mode in order:
            if mode == "maintenance_debug":
                for slot in range(32):
                    path = root / f"snapshot-{slot:02}.json"
                    path.write_bytes(retained_bytes if path == retain else b"x" * (1024 * 1024))
                before = sum(path.stat().st_size for path in root.iterdir() if path.is_file())
                started = time.perf_counter()
                worker = subprocess.Popen(
                    [
                        "nice",
                        "-n",
                        "10",
                        str(binary),
                        "--internal-cache-maintenance",
                        str(root),
                        retain.name,
                    ],
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                    env=env,
                )
                worker_result: dict[str, float] = {}

                collector = threading.Thread(
                    target=collect_worker, args=(worker, worker_result, started)
                )
                collector.start()
                foreground_offset = (time.perf_counter() - started) * 1000
                sample = foreground(binary, repo, env)
                collector.join()
                report = json.loads(
                    json.loads((root / "maintenance-report.json").read_text())["payload"]
                )
                sample.update(
                    worker=worker_result,
                    worker_report=report,
                    cache_bytes_before=before,
                    worker_overlapped_foreground=worker_result["elapsed_ms"] > foreground_offset,
                )
            else:
                sample = foreground(binary, repo, env, terminal=mode == "pty_debug")
            results[mode].append(sample)
    report = {
        "platform": platform.platform(),
        "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
        "repeats": args.repeats,
        "samples": results,
        "median_ms": {
            name: statistics.median(s["elapsed_ms"] for s in samples)
            for name, samples in results.items()
        },
    }
    (args.directory / "runtime-results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["median_ms"], indent=2))


if __name__ == "__main__":
    main()

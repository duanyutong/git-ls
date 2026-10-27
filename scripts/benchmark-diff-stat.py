#!/usr/bin/env -S uv run
"""Measure per-layer statistics with isolated repositories and retained raw evidence.

Application caches are separate for each binary; filesystem caches remain warm.
Run without other builds or benchmarks to obtain comparable timing samples.
"""

import argparse
import errno
import hashlib
import json
import os
import pathlib
import platform
import random
import shutil
import statistics
import subprocess
import tempfile
import time
from dataclasses import dataclass
from typing import TypedDict

ENV = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
ENV.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1", GIT_ATTR_NOSYSTEM="1", LC_ALL="C")


@dataclass(frozen=True)
class Case:
    name: str
    layers: int
    shape: str = "independent"
    renames: int = 0


CASES = [
    Case("independent-1", 1),
    Case("independent-8", 8),
    Case("independent-32", 32),
    Case("independent-128", 128),
    Case("stack-32", 32, "stack"),
    Case("shared-16-plus-32", 32, "shared"),
    Case("empty-32", 32, "empty"),
    Case("orphans-8", 8, "orphan"),
    Case("renames-200", 1, renames=200),
    Case("renames-1000", 1, renames=1000),
    Case("large-text", 1, "large"),
    Case("binary", 1, "binary"),
    Case("metadata-600", 600, "metadata"),
]
MODES = {
    "warm_off": ["--no-diff-stat"],
    "warm_on": ["--diff-stat"],
    "uncached_off": ["--no-cache", "--no-diff-stat"],
    "uncached_on": ["--no-cache", "--diff-stat"],
    "cold_on": ["--diff-stat"],
    "refresh_on": ["--refresh-cache", "--diff-stat"],
    "low_on": ["--verbosity", "0", "--diff-stat"],
}


class Sample(TypedDict):
    elapsed_ms: float
    peak_rss_bytes: int | None
    cpu_ms: float | None


def git(repo: pathlib.Path, *args: str, data: bytes | None = None) -> bytes:
    return subprocess.run(
        ["git", "-C", str(repo), *args], input=data, env=ENV, capture_output=True, check=True
    ).stdout


def fixture(directory: pathlib.Path, case: Case) -> pathlib.Path:
    repo = directory / case.name
    repo.mkdir(parents=True)
    git(repo, "-c", "init.templateDir=", "init", "-q", "-b", "main")
    git(repo, "config", "gc.auto", "0")
    git(repo, "config", "core.attributesFile", os.devnull)
    records = bytearray()
    mark = 0

    def commit(ref: str, parent: int, files: list[tuple[str, bytes]], delete: bool = False) -> int:
        nonlocal mark
        mark += 1
        records.extend(
            f"commit refs/heads/{ref}\nmark :{mark}\n"
            f"committer Benchmark <benchmark@example.invalid> {1700000000 + mark} +0000\n"
            "data 6\nlayer\n".encode()
        )
        records.extend(f"from :{parent}\n".encode() if parent else b"deleteall\n")
        if delete:
            records.extend(b"deleteall\n")
        for path, content in files:
            records.extend(f"M 100644 inline {path}\ndata {len(content)}\n".encode())
            records.extend(content + b"\n")
        records.extend(b"\n")
        return mark

    initial = [
        (f"old-{index:05}.txt", f"file {index}\n".encode() + b"stable\n" * 30)
        for index in range(case.renames)
    ]
    if case.shape == "large":
        initial = [("data", b"stable content\n" * 200000)]
    base = commit("main", 0, initial or [("data", b"base\n")])
    trunk = base
    if case.shape == "shared":
        for index in range(16):
            trunk = commit(f"shared-{index:03}", trunk, [(f"shared-{index}", b"content\n")])
    for index in range(case.layers):
        parent = trunk if case.shape in ("stack", "shared") else base
        if case.shape == "orphan":
            parent = 0
        files = [(f"layer-{index:03}", b"changed\n")]
        if case.renames:
            files = [
                (f"new-{number:05}.txt", content + b"changed\n")
                for number, (_, content) in enumerate(initial)
            ]
        elif case.shape in ("empty", "metadata"):
            files = []
        elif case.shape == "large":
            files = [("data", initial[0][1] + b"changed\n")]
        elif case.shape == "binary":
            files = [("binary", b"\0binary\0content\0")]
        tip = commit(f"topic-{index:03}", parent, files, bool(case.renames))
        if case.shape == "stack":
            trunk = tip
    git(repo, "fast-import", "--quiet", data=bytes(records))
    return repo


def measure(
    binary: pathlib.Path,
    repo: pathlib.Path,
    backend: str,
    flags: list[str],
    trace: pathlib.Path | None = None,
    width: int | None = None,
) -> tuple[Sample, bytes, str]:
    command = [
        str(binary),
        "--backend",
        backend,
        *([] if "--verbosity" in flags else ["--verbosity", "2"]),
        "--layout",
        "columns",
        "--color",
        "never",
        "--no-cache-maintenance",
        *flags,
    ]
    environment = ENV | ({"GIT_TRACE2_EVENT": str(trace)} if trace else {})
    master, slave = None, None
    if width is not None:
        import fcntl
        import pty
        import struct
        import termios

        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as diagnostics:
        started = time.perf_counter()
        child = subprocess.Popen(
            command,
            cwd=repo,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=slave if slave is not None else output,
            stderr=diagnostics,
        )
        if slave is not None:
            os.close(slave)
        if master is not None:
            try:
                while chunk := os.read(master, 65536):
                    output.write(chunk)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
            finally:
                os.close(master)
        if hasattr(os, "wait4"):
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            rss = usage.ru_maxrss * (1 if platform.system() == "Darwin" else 1024)
            cpu = (usage.ru_utime + usage.ru_stime) * 1000
        else:
            child.wait()
            rss, cpu = None, None
        elapsed = (time.perf_counter() - started) * 1000
        output.seek(0)
        diagnostics.seek(0)
        stdout = output.read()
        debug = diagnostics.read().decode(errors="replace")
        if child.returncode:
            raise RuntimeError(f"{command}: {debug}")
    return {"elapsed_ms": elapsed, "peak_rss_bytes": rss, "cpu_ms": cpu}, stdout, debug


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", type=pathlib.Path)
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--backend", choices=["gix", "shell"], default="gix")
    parser.add_argument("--repeats", type=int, default=9)
    parser.add_argument("--cases", nargs="+", choices=[case.name for case in CASES])
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    args.directory.mkdir(parents=True, exist_ok=True)
    binaries = {"candidate": args.candidate.resolve()}
    if args.baseline:
        binaries["baseline"] = args.baseline.resolve()
    report = {
        "platform": platform.platform(),
        "backend": args.backend,
        "repeats": args.repeats,
        "git_version": subprocess.check_output(["git", "--version"], text=True).strip(),
        "binaries": {
            name: {
                "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            }
            for name, binary in binaries.items()
        },
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="git-ls-diff-bench-") as scratch:
        for case in CASES:
            if args.cases and case.name not in args.cases:
                continue
            repos = {name: fixture(pathlib.Path(scratch) / name, case) for name in binaries}
            modes = {
                name: flags
                for name, flags in MODES.items()
                if case.shape != "metadata" or name.endswith("off")
            }
            outputs: dict[str, bytes] = {}
            samples: dict[str, list[Sample]] = {}
            for name, binary in binaries.items():
                for mode, flags in modes.items():
                    _, output, _ = measure(binary, repos[name], args.backend, flags)
                    if mode in outputs and outputs[mode] != output:
                        raise AssertionError(f"output mismatch: {case.name} {mode}")
                    outputs[mode] = output
                    samples[f"{name}.{mode}"] = []
            order = [(name, mode) for name in binaries for mode in modes]
            for repetition in range(args.repeats):
                random.Random(repetition).shuffle(order)
                for name, mode in order:
                    if mode == "cold_on":
                        shutil.rmtree(repos[name] / ".git/git-ls/cache", ignore_errors=True)
                    sample, output, _ = measure(
                        binaries[name], repos[name], args.backend, modes[mode]
                    )
                    if output != outputs[mode]:
                        raise AssertionError(f"unstable output: {case.name} {name}.{mode}")
                    samples[f"{name}.{mode}"].append(sample)
            probes = {}
            probe_outputs: dict[str, bytes] = {}
            for name, binary in binaries.items():
                for mode in [*modes, *(["narrow_on"] if os.name == "posix" else [])]:
                    if mode == "cold_on":
                        shutil.rmtree(repos[name] / ".git/git-ls/cache", ignore_errors=True)
                    trace = (args.directory / f"{case.name}.{name}.{mode}.trace.jsonl").resolve()
                    trace.unlink(missing_ok=True)
                    probe_sample, probe_output, debug = measure(
                        binary,
                        repos[name],
                        args.backend,
                        [*modes.get(mode, ["--diff-stat"]), "--debug"],
                        trace,
                        width=20 if mode == "narrow_on" else None,
                    )
                    if mode in probe_outputs and probe_outputs[mode] != probe_output:
                        raise AssertionError(f"probe output mismatch: {case.name} {mode}")
                    probe_outputs[mode] = probe_output
                    (args.directory / f"{case.name}.{name}.{mode}.debug.txt").write_text(debug)
                    events = [json.loads(line) for line in trace.read_text().splitlines()]
                    probes[f"{name}.{mode}"] = {
                        "sample": probe_sample,
                        "stdout_sha256": hashlib.sha256(probe_output).hexdigest(),
                        "git_argv": [
                            event["argv"] for event in events if event.get("event") == "start"
                        ],
                        "counters": [
                            line for line in debug.splitlines() if "perf counters" in line
                        ],
                    }
            result = {
                "fixture": case.name,
                "layers": case.layers,
                "shape": case.shape,
                "renamed_files": case.renames,
                "configuration": git(
                    next(iter(repos.values())), "config", "--null", "--list"
                ).decode(),
                "modes": {
                    name: {
                        "median_ms": statistics.median(sample["elapsed_ms"] for sample in values),
                        "samples": values,
                    }
                    for name, values in samples.items()
                },
                "probes": probes,
            }
            report["cases"].append(result)
            (args.directory / "results.json").write_text(json.dumps(report, indent=2) + "\n")
            print(
                case.name,
                {name: round(value["median_ms"], 2) for name, value in result["modes"].items()},
                flush=True,
            )


if __name__ == "__main__":
    main()

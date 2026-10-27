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
import re
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
    trunk: int = 16
    paths: int = 0


CASES = [
    Case("independent-1", 1),
    Case("independent-8", 8),
    Case("independent-32", 32),
    Case("independent-128", 128),
    Case("independent-512", 512),
    Case("stack-32", 32, "stack"),
    Case("shared-16-plus-32", 32, "shared"),
    Case("empty-32", 32, "empty"),
    Case("orphans-8", 8, "orphan"),
    Case("renames-200", 1, renames=200),
    Case("renames-1000", 1, renames=1000),
    Case("large-text", 1, "large"),
    Case("binary", 1, "binary"),
    Case("metadata-600", 600, "metadata"),
    Case("shared-named-320", 32, "shared_named", trunk=320),
    Case("shared-deep-5000", 32, "shared_deep", trunk=5000),
    Case("paths-4000", 1, "paths", paths=4000),
    Case("paths-6000", 1, "paths", paths=6000),
    Case("amend-identical-tree", 1, "amend"),
    Case("edit-same-path", 32, "edit"),
    Case("edit-new-path", 32, "new_path"),
    Case("new-topic", 32, "new_topic"),
    Case("unrelated-config", 32, "config"),
    Case("relevant-attributes", 32, "attributes"),
    Case("inactive-path-attributes", 2, "inactive"),
    Case("alternating-worktrees", 32, "worktree"),
    Case("branchless-interior-32", 1, "branchless", trunk=32),
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
    if case.shape.startswith("shared") or case.shape == "branchless":
        for index in range(case.trunk):
            name = "shared" if case.shape in ("shared_deep", "branchless") else f"shared-{index:03}"
            files = (
                [(f"shared-{index}", b"content\n")]
                if case.shape == "shared"
                else [("shared-data", b"content\n")]
            )
            trunk = commit(name, trunk, files)
    for index in range(case.layers):
        parent = (
            trunk
            if case.shape == "stack"
            or case.shape.startswith("shared")
            or case.shape == "branchless"
            else base
        )
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
        elif case.shape == "paths":
            files = [
                (f"file-{number:05}-" + "x" * 210, b"changed\n") for number in range(case.paths)
            ]
        tip = commit(f"topic-{index:03}", parent, files, bool(case.renames))
        if case.shape == "stack":
            trunk = tip
    git(repo, "fast-import", "--quiet", data=bytes(records))
    if case.shape == "worktree":
        git(repo, "worktree", "add", "--detach", str(repo.parent / "linked"), "main")
    elif case.shape == "inactive":
        git(repo, "update-ref", "refs/benchmark/inactive-tip", "topic-001")
    elif case.shape == "branchless":
        git(repo, "branchless", "init", "--main-branch", "main")
    return repo


def prepare_sample(
    binary: pathlib.Path, repo: pathlib.Path, case: Case, backend: str, flags: list[str], token: int
) -> None:
    """Arrange one development event outside the measured invocation."""
    if case.shape == "worktree":
        measure(binary, repo.parent / "linked", backend, flags)
    elif case.shape == "config":
        git(repo, "config", "core.editor", f"unused-editor-{token}")
    elif case.shape in ("attributes", "inactive"):
        directory = repo / ".git/info"
        directory.mkdir(exist_ok=True)
        path = "layer-000" if case.shape == "attributes" else "layer-001"
        if case.shape == "inactive":
            git(repo, "update-ref", "refs/heads/topic-001", "refs/benchmark/inactive-tip")
        previous = "diff" if token % 2 else "-diff"
        (directory / "attributes").write_text(f"{path} {previous}\n")
        measure(binary, repo, backend, flags)
        if case.shape == "inactive":
            git(repo, "update-ref", "-d", "refs/heads/topic-001")
            measure(binary, repo, backend, flags)
        current = "-diff" if token % 2 else "diff"
        (directory / "attributes").write_text(f"{path} {current}\n")
    elif case.shape in ("amend", "edit", "new_path", "new_topic"):
        branch = "added-topic" if case.shape == "new_topic" else "topic-000"
        if case.shape == "new_topic":
            git(repo, "update-ref", "-d", f"refs/heads/{branch}")
            measure(binary, repo, backend, flags)
        parent = git(repo, "rev-parse", "main").decode().strip()
        if case.shape == "amend":
            path, content = "layer-000", b"changed\n"
        elif case.shape == "new_path":
            path, content = f"changed-{token}", b"changed\n"
        else:
            path, content = "layer-000", f"changed {token}\n".encode()
        record = (
            f"commit refs/heads/{branch}\n"
            f"committer Benchmark <benchmark@example.invalid> {1700000500 + token} +0000\n"
            f"data 6\nlayer\nfrom {parent}\nM 100644 inline {path}\ndata {len(content)}\n"
        ).encode()
        git(repo, "fast-import", "--quiet", "--force", data=record + content + b"\n")


def measure(
    binary: pathlib.Path,
    repo: pathlib.Path,
    backend: str,
    flags: list[str],
    trace: pathlib.Path | None = None,
    width: int | None = None,
    colour: str = "never",
) -> tuple[Sample, bytes, str]:
    command = [
        str(binary),
        "--backend",
        backend,
        *([] if "--verbosity" in flags else ["--verbosity", "2"]),
        "--layout",
        "columns",
        "--color",
        colour,
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
        attributes = termios.tcgetattr(slave)
        attributes[1] &= ~termios.OPOST
        termios.tcsetattr(slave, termios.TCSANOW, attributes)
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


def visible_style_digest(output: bytes) -> str:
    """Fingerprint non-space glyphs and their styles, ignoring invisible control changes."""
    digest = hashlib.sha256()
    state: list[bytes] = []
    position = 0

    def record(text: bytes) -> None:
        for char in text.decode():
            if not char.isspace():
                digest.update(char.encode())
                digest.update(repr(state).encode())

    for match in re.finditer(rb"\x1b\[[0-9;]*m", output):
        record(output[position : match.start()])
        code = match.group()
        if code in (b"\x1b[0m", b"\x1b[m"):
            state.clear()
        else:
            state.append(code)
        position = match.end()
    record(output[position:])
    return digest.hexdigest()


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
                name: [*flags, *(["all()"] if case.shape == "branchless" else [])]
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
            changing_outputs: dict[str, bytes] = {}
            for repetition in range(args.repeats):
                random.Random(repetition).shuffle(order)
                for name, mode in order:
                    if mode == "cold_on":
                        shutil.rmtree(repos[name] / ".git/git-ls/cache", ignore_errors=True)
                    token = repetition * len(modes) + list(modes).index(mode)
                    prepare_sample(
                        binaries[name], repos[name], case, args.backend, modes[mode], token
                    )
                    sample, output, _ = measure(
                        binaries[name], repos[name], args.backend, modes[mode]
                    )
                    if case.shape in (
                        "amend",
                        "edit",
                        "new_path",
                        "new_topic",
                        "attributes",
                        "inactive",
                    ):
                        key = f"{repetition}.{mode}"
                        expected = changing_outputs.setdefault(key, output)
                    else:
                        expected = outputs[mode]
                    if output != expected:
                        raise AssertionError(f"unstable output: {case.name} {name}.{mode}")
                    samples[f"{name}.{mode}"].append(sample)
            probes = {}
            probe_outputs: dict[str, bytes] = {}
            probe_styles: dict[str, str] = {}
            for name, binary in binaries.items():
                terminal_modes = (
                    ["narrow_on", "wide_colour_on", "wide_colour_off"] if os.name == "posix" else []
                )
                for mode in [*modes, *terminal_modes]:
                    if mode == "cold_on":
                        shutil.rmtree(repos[name] / ".git/git-ls/cache", ignore_errors=True)
                    trace = (args.directory / f"{case.name}.{name}.{mode}.trace.jsonl").resolve()
                    trace.unlink(missing_ok=True)
                    probe_flags = modes.get(
                        mode, ["--no-diff-stat"] if mode.endswith("off") else ["--diff-stat"]
                    )
                    if mode not in modes and case.shape == "branchless":
                        probe_flags = [*probe_flags, "all()"]
                    prepare_sample(
                        binary,
                        repos[name],
                        case,
                        args.backend,
                        probe_flags,
                        100000 + [*modes, *terminal_modes].index(mode),
                    )
                    probe_sample, probe_output, debug = measure(
                        binary,
                        repos[name],
                        args.backend,
                        [*probe_flags, "--debug"],
                        trace,
                        width=20
                        if mode == "narrow_on"
                        else 120
                        if mode.startswith("wide_colour")
                        else None,
                        colour="always" if mode.startswith("wide_colour") else "never",
                    )
                    visible = re.sub(rb"\x1b\[[0-9;]*m", b"", probe_output)
                    if mode in probe_outputs and probe_outputs[mode] != visible:
                        raise AssertionError(f"probe output mismatch: {case.name} {mode}")
                    probe_outputs[mode] = visible
                    style_digest = (
                        visible_style_digest(probe_output)
                        if mode.startswith("wide_colour")
                        else None
                    )
                    if style_digest is not None:
                        if mode in probe_styles and probe_styles[mode] != style_digest:
                            raise AssertionError(f"visible colour mismatch: {case.name} {mode}")
                        probe_styles[mode] = style_digest
                    (args.directory / f"{case.name}.{name}.{mode}.debug.txt").write_text(debug)
                    events = [json.loads(line) for line in trace.read_text().splitlines()]
                    probes[f"{name}.{mode}"] = {
                        "sample": probe_sample,
                        "stdout_sha256": hashlib.sha256(probe_output).hexdigest(),
                        "visible_stdout_sha256": hashlib.sha256(visible).hexdigest(),
                        "visible_style_sha256": style_digest,
                        "stdout_bytes": len(probe_output),
                        "ansi_sequences": probe_output.count(b"\x1b["),
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

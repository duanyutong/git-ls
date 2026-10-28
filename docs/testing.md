# Validation

Tests exercise graph, configuration, cache and rendering policy through unit tests and disposable Git repositories.
Both native and subprocess backends are covered.
Application configuration, commit identities and credentials are isolated from the developer's repositories.

`just check` runs repository hygiene, Clippy, dependency policy, unused-dependency detection, Python checks, every test target, separate doctests and the production release build.
Run `just check` and `just coverage` explicitly before committing changes for acceptance.
Pre-commit hooks provide formatting and hygiene checks; pre-push verifies version history and performs lightweight, read-only built-in file checks.
CI independently runs `just check` on Linux and macOS and `just coverage` on Linux.
`just coverage` requires 100% executable line, region and function coverage within the Rust unit sources declared in [coverage-policy.toml](../coverage-policy.toml), plus the separate versioning sanity guard.
Filesystem, compiled-binary and process adapters retain their behavioural tests within `just check`.
The [coverage policy](coverage-policy.md) specifies source classification, test selection, profile isolation and report acceptance.
`just test-unit` runs only in-memory unit tests; `just coverage-unit` measures their production coverage in freshly cleaned, independent profile directories.
Repository-backed internal tests use the `repositories` module namespace and remain part of ordinary acceptance.
Test implementations reside in separate source files and are excluded from production coverage reports.
Coverage reports reside under `coverage/`; measured gate timings reside under `.validation/`.

The [performance report](performance.md) specifies reproducible benchmark commands and their measurement limits.
Benchmark scripts use disposable repositories and retain raw samples.
Plain `just check` and `prek` require no repository-specific `PATH` overrides after setup.

## Version automation

The `scripts/test_version.py` suite exercises Conventional Commit policy, generated increments, formatting preservation, staged and unstaged changes, initial commits, amendments, push revisions and annotated tags in disposable repositories.
Tag-publication tests use private local bare remotes, including failed publication, repeated invocation, published baselines, unrelated tags and conflicts.
The tests use standard-library `unittest` and form part of `just test`.
The `scripts/test_workflows.py` suite verifies publication dependencies, event eligibility, exact revision selection, permissions, concurrency and the single automatic publication path.
Its disposable Git repositories exercise the actual publication shell guard against descendant advances and history rewrites, and verify that command-scoped authentication does not persist credentials.
Workflow tests also form part of `just test`; they publish no production tags.
The `just coverage-versioning` sanity guard uses coverage.py to require at least 95% line coverage of `scripts/version.py`, including its command subprocesses.
A passing guard reports its result without displaying its measured percentage; failure displays missing-line diagnostics, and detailed reports remain available under `coverage/`.
The guard forms part of `just coverage` and does not contribute to the Rust unit percentage.

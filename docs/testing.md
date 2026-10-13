# Validation

Tests exercise graph, configuration, cache and rendering policy through unit tests and disposable Git repositories.
Both native and subprocess backends are covered.
Application configuration, commit identities and credentials are isolated from the developer's repositories.

`just check` runs repository hygiene, Clippy, dependency policy, unused-dependency detection, Python checks, every test target, separate doctests and the production release build.
Run `just check` and `just coverage` explicitly before committing changes for acceptance.
Pre-commit hooks provide formatting and hygiene checks; pre-push verifies version history and performs lightweight, read-only built-in file checks.
CI independently runs `just check` on Linux and macOS and `just coverage` on Linux.
`just coverage` enforces at least 95% package line coverage and complete line, region and function coverage within the unit sources declared in [coverage-policy.toml](../coverage-policy.toml).
Filesystem, compiled-binary and process adapters remain covered by the package gate and integration tests.
The shared [coverage policy](coverage-policy.md) specifies source classification, test selection, profile isolation and report acceptance.
`just test-unit` runs only in-memory unit tests; `just coverage-unit` measures their production coverage in freshly cleaned, independent profile directories.
Repository-backed internal tests use the `repositories` module namespace and remain part of ordinary and package acceptance.
Test implementations reside in separate source files and are excluded from production coverage reports.
Coverage reports reside under `coverage/`; measured gate timings reside under `.validation/`.

The [performance report](performance.md) specifies reproducible benchmark commands and their measurement limits.
Benchmark scripts use disposable repositories and retain raw samples.
Plain `just check` and `prek` require no repository-specific `PATH` overrides after setup.

## Version automation

The `xtask` tests exercise generated version increments, staged and unstaged changes, initial commits, amendments, push revisions and annotated tags in disposable repositories.
Tag-publication tests use private local bare remotes.
The package gate requires at least 95% line coverage; the manifest and version-policy modules require complete line, region and function coverage from their independent unit suite.
The generated Clap CLI schema retains explicit in-memory parser assertions but has no executable production coverage entry.
These gates form part of `just coverage`.

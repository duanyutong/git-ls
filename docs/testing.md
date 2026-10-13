# Validation

Tests exercise graph, configuration, cache and rendering policy through unit tests and disposable Git repositories.
Both native and subprocess backends are covered.
Application configuration, commit identities and credentials are isolated from the developer's repositories.

`just check` runs repository hygiene, Clippy, dependency policy, unused-dependency detection, Python checks, every test target, separate doctests and the production release build.
`just coverage` enforces at least 95% package line coverage and complete line, region and function coverage within the named unit boundary in `justfile`.
Filesystem, compiled-binary and process adapters remain covered by the package gate and integration tests.
Coverage reports reside under `coverage/`; measured gate timings reside under `.validation/`.

The [performance report](performance.md) specifies reproducible benchmark commands and their measurement limits.
Benchmark scripts use disposable repositories and retain raw samples.
Plain `just check` and `prek` require no repository-specific `PATH` overrides after setup.

## Version automation

The `xtask` tests exercise generated version increments, staged and unstaged changes, initial commits, amendments, push revisions and annotated tags in disposable repositories.
Tag-publication tests use private local bare remotes.
The package gate requires at least 95% line coverage; the CLI, manifest and version-policy modules require complete line, region and function coverage.
These gates form part of `just coverage`.

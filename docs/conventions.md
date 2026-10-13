# Engineering conventions

This document specifies the engineering requirements for this repository.
The checkout provisions its own development tools.

| Area | Convention |
| --- | --- |
| Compiler | Rust 1.99.0, Edition 2024, minimal toolchain with rustfmt, Clippy, LLVM tools and rust-analyzer. |
| Rust policy | Workspace-wide compiler metadata and lints, inherited by the application; unsafe code is forbidden. |
| Release profile | Optimisation level 3, fat LTO, one code-generation unit, aborting panics and stripped symbols. |
| Build identity | Version output records the package version, commit, dirty state, target, compiler and build timestamp. |
| Dependencies | Committed Cargo lockfile; locked builds and tests; current advisory, licence, source and duplicate-version checks. |
| Setup | `sh setup.sh` or `just setup`; Python through uv and additional development tools through prek-managed environments. |
| Formatting | Rustfmt, cargo-sort, offline Tombi, Ruff, spelling and repository hygiene. |
| Hooks | Pre-commit formatting and hygiene, Conventional Commit and version validation at commit time, version-history verification and lightweight, read-only built-in file checks before push. |
| Validation | `just lint`, `just typecheck`, `just test`, `just coverage` and `just build-release`; `just check` combines lint, tests, doctests and release compilation. |
| Coverage | At least 95% application line coverage and complete coverage within the declared pure-policy boundaries. |
| CI | Linux and macOS, pinned action revisions, repository setup and local recipes, bounded jobs and retained evidence. |
| Configuration | Git owns parsing, includes, precedence and Boolean conversion; keys use camelCase and flags use kebab-case. |
| Release policy | Commit hooks generate required version increments; successful main CI triggers annotated version tags. |
| Documentation | Concise README, copyable Git configuration, separate command and development references, British English and one sentence per source line. |

Dependencies, backend fixtures and coverage boundaries remain explicit.
Licence allowlists, duplicate-version exceptions and Clippy exceptions require justification for the actual dependency graph or code surface.
The release procedure is specified in [versioning](versioning.md).

## Module organisation

The application entry point owns argument resolution, repository discovery and execution orchestration.
Lane selection, construction and grouping reside in separate submodules beneath `lanes`.
Analysis receives repository capabilities and an observer; its policy shall not obtain clocks or write standard streams.
The diagnostics adapter owns timing and output, whilst scope notifications preserve attribution on successful and failed execution.

The cache module owns reusable facts, recency retention and compatibility metadata.
Cache keys use explicit JSON representations of compatibility and canonical selection inputs; diagnostic `Debug` formatting is not a persistence contract.
The analysis semantics version shall change when graph interpretation, backend behaviour or stored answers become incompatible, including relevant dependency upgrades.
Unrepresentable keys bypass persistent reuse without lossy path conversion.

Cache scheduling and eviction decisions accept observed state and time through pure policy functions.
Filesystem access, locking, clocks and process creation remain in the maintenance adapter.
The strict unit coverage boundary includes the extracted policy, compatibility and fact-retention modules.

Production ancestry traits require collective tip selection, independent-head discovery and batch main-distance analysis.
Implementations shall provide those capabilities explicitly.
Slow reference algorithms exist solely as opted-in test utilities and cannot be inherited by a production backend.

# Development

Rust 1.99.0 and Edition 2024 are specified by `rust-toolchain.toml` and `Cargo.toml`.
Development requires Git, rustup, uv, just and prek; command-runner versions are declared in `scripts/dev-tools.env`.
Linux requires a C compiler and pkg-config; the corresponding Debian and Ubuntu packages are `build-essential pkg-config`.
macOS requires the Xcode command-line tools.

The command `sh setup.sh` verifies the command-runner versions, synchronises the Python environment from `uv.lock`, provisions hook environments, fetches locked Rust dependencies and registers hooks.
prek owns additional development-tool installations in its standard cache; setup creates no repository-local executable directory.
The non-package project in `pyproject.toml` pins Python to 3.15.0 solely for development scripts.
uv selects or downloads a compatible interpreter using its standard storage and maintains the repository environment in `.venv`.
Run development scripts as `uv run scripts/<name>.py`; executable scripts also provide `#!/usr/bin/env -S uv run` shebangs.
Run these commands from the repository so uv discovers its Python requirement.
prek manages the pinned Ruff, Pyright, Tombi, ShellCheck and typos distributions in isolated hook environments declared in `prek.toml`.
It also provisions cargo-sort, cargo-deny, cargo-machete, cargo-llvm-cov and git-branchless as Rust CLI dependencies of the hooks that require them.
rustup supplies the repository compiler, rustfmt and Clippy components.
The hook interpreter and compiler requests must remain consistent with `project.requires-python` and `rust-toolchain.toml` when upgrading either runtime.
Use `prek run ruff-check --all-files` for Python linting, `prek run ruff-format --all-files` for Python formatting and `prek run pyright --all-files --hook-stage manual` for Python type checking.
Hooks and recipes require no repository-specific `PATH` changes or virtual-environment activation.
The recipe `just test` runs acceptance through a managed hook containing the declared git-branchless dependency.
Coverage recipes similarly run their collectors inside hooks containing cargo-llvm-cov and, where repository tests require it, git-branchless.
Manual lint selects its own hooks explicitly, so invoking it does not execute acceptance tests or coverage.
CI obtains the pinned just and prek command runners through `uv run` and caches prek environments alongside the Cargo caches.
Tool installation and update behaviour is delegated to prek rather than maintained in repository-specific installers.

The commands `just typecheck` and `just test-unit` provide focused checks during development.
The recipe `just fmt` applies formatting through prek and reports changes for review.
The recipe `just lint` runs hygiene followed by the manual Clippy, dependency-policy, unused-dependency and Python type checks.
The recipe `just check` runs lint, all test targets, separate doctests and the production release build.
Run `just check` and `just coverage` before committing changes for acceptance.
These commands remain explicit validation gates and are also executed by CI; push hooks do not invoke them.
The release profile uses optimisation level 3, fat link-time optimisation, one code-generation unit, aborting panics and stripped symbols for release binaries.
Fat LTO permits optimisation across crate boundaries; one code-generation unit permits optimisation across the crate.
These settings can improve code size or execution speed, but increase build time and do not guarantee a runtime improvement.
Symbol stripping reduces executable metadata and limits subsequent debugging.
Aborting panics terminate the process without stack unwinding or destructor execution; ordinary `Result` error handling remains unchanged.
Optimisation level 3 is already the [Cargo release default](https://doc.rust-lang.org/cargo/reference/profiles.html).

Measured validation recipes record elapsed time and exit status under `.validation/`; failed attempts remain in the record.
The directory contains disposable validation evidence and scratch data, and may be deleted without removing tool installations.
The default test concurrency is six and may be overridden with `RUST_TEST_THREADS`.
Formatter rewrites require review and a subsequent clean hygiene run.

Compiler and development-tool upgrades require review of the lockfile, lint results, behavioural tests and validation timings.
Dependency exceptions require a crate-specific justification in `deny.toml`.
Commit hooks validate Conventional Commits and generate required package-version increments.
Pre-commit hooks perform formatting, spelling, syntax and repository-hygiene checks on applicable staged files.
Pre-push verifies committed version history and runs prek's built-in trailing-whitespace, end-of-file and large-file checks.
The whitespace and end-of-file hooks apply automatic fixes at pre-commit and use `--check` at pre-push to report defects without modifying files.
It invokes the version helper through Cargo, which may compile that helper when its sources or dependencies have changed, but does not run application linting, tests, coverage or release compilation.
Existing hook installations read `prek.toml` on invocation and require no reinstallation for this policy change.
Successful CI on `main` triggers annotated version tags, as specified in [versioning](versioning.md).

The engineering policy is specified in [engineering conventions](conventions.md).

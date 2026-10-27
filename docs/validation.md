# Acceptance record

Entries record requirements and results at their stated dates; the current acceptance contract is defined in the [coverage policy](coverage-policy.md).

## Core Rust unit acceptance: 26 October 2026

Version 0.15.0 has been validated on macOS arm64 with Rust 1.99.1 and Python 3.15.0.
`just check` and `just coverage` pass, including all 298 Rust acceptance tests, 16 versioning tests, 15 collector and guard regression tests, separate doctests and production release compilation.
The independent Rust unit run executes 242 tests and covers all 3,315 executable lines, 5,052 regions and 384 functions across the unchanged 35-module unit scope.
The 22 repository-backed internal tests remain part of ordinary acceptance and do not contribute profiles to unit coverage.
The separate versioning sanity guard passes its 95% line minimum, with its measured percentage retained in diagnostic reports.
Aggregate application coverage has been retired, and coverage acceptance requires only the pinned stable compiler.
Evidence resides in `coverage/unit/summary.json`, the associated unit and versioning reports, `.validation/unit-policy-check.log` and `.validation/unit-policy-coverage.log`.

## Managed development tools: 13 October 2026

prek provisions ShellCheck and typos from their hook repositories, and additional Cargo executables from the dependencies declared in `prek.toml`.
Acceptance tests and coverage collectors execute within managed environments containing the required git-branchless and cargo-llvm-cov distributions.
The repository-specific asset installer, installation manifest and executable-path wrapper have been removed.
Initial and repeated `sh setup.sh`, `just fmt`, `just check` and `just coverage` pass on macOS arm64 after deleting `.validation/tools`.
Acceptance deliberately places rejecting wrappers ahead of global ShellCheck, typos, cargo-sort, cargo-deny, cargo-machete, cargo-llvm-cov and git-branchless commands, establishing that the hooks supply their required executables.
The command-runner bootstrap used by CI also produces the declared just and prek versions through `uv run`.
Evidence resides in `.validation/prek-setup.log`, `.validation/prek-setup-repeat.log`, `.validation/prek-fmt.log`, `.validation/prek-check.log` and `.validation/prek-coverage.log`.
The remaining `.validation` contents are disposable evidence and scratch data; deleting the directory does not remove active tool installations.
Hosted Linux and macOS CI execution remains unverified locally.

## Python tooling migration: 13 October 2026

Development scripts execute through uv under the Python 3.15.0 requirement in `pyproject.toml`, with the repository lockfile `uv.lock` and an ignored `.venv` environment.
prek provisions the pinned Ruff, Pyright and Tombi hook environments independently of repository-local executable links.
At this stage, the obsolete Python bootstrap, tool environments and interpreter links had been removed from the local `.validation/tools` installation, whilst native development executables remained in that directory.
Initial and repeated `sh setup.sh`, `just fmt`, `just check` and `just coverage` pass on macOS arm64.
The first concurrent coverage attempt failed an existing cache-hit integration assertion; both the subsequent single-worker run and the repeated default six-worker run pass.
No Rust source or acceptance threshold has been changed for this migration.
Evidence resides in `.validation/uv-setup.log`, `.validation/uv-setup-repeat.log`, `.validation/uv-fmt.log`, `.validation/uv-config-check.log`, `.validation/uv-check.log`, `.validation/uv-coverage.log`, `.validation/uv-coverage-serial.log` and `.validation/uv-coverage-retry.log`.
CI installs the pinned uv release through the official setup action; hosted Linux and macOS execution remains unverified locally.

## Graph and cache boundary remediation: 13 October 2026

Version 0.13.4 has been validated on macOS arm64 with Rust 1.99.0 and Python 3.15.0.
The five confirmed follow-up findings have been addressed whilst preserving the then-current coverage policy and separated test implementation files.
Disposable-repository regressions cover colliding tag and branch names, symbolic local aliases, replacement and graft parentage, invalid argument encoding, oversized metadata and topology, and bounded maintenance progression.
Exact topology reuse remains available independently of optional metadata admission, and unchanged warm hits omit cache revalidation and publication.
The analysis compatibility identifier is `analysis-3`; earlier graph snapshots and facts are not reused across this semantic change.

`just check` passes with 270 application tests, 41 versioning tests, eight collector regression tests, separate doctests and production release compilation.
The application unit suite executes 218 in-memory tests; 22 repository-backed internal tests remain in ordinary and package acceptance.
All 3,011 lines, 4,534 regions and 351 functions in its 31 declared unit modules are covered independently.
Application package coverage is 4,011 of 4,082 lines, or 98.26%.
Versioning package coverage remains 382 of 390 lines, or 97.95%; its two unit modules retain complete coverage of 107 lines, 166 regions and 15 functions.
`just coverage` passes without changing thresholds or source classifications.
The follow-up plan and acceptance logs reside in `.agents/plans/follow-up-issues.md` and its adjacent evidence directory.
Linux and other platform-specific execution remain obligations of the corresponding CI jobs.

## Coverage inventory: 13 October 2026

At this stage, the repository adopted an explicit manifest schema, collectors and collector regression checks.
Every production source under the declared roots is classified exactly once, with a 95% package line minimum and complete independent coverage of declared unit sources.
The application unit run executes 217 tests using values and in-memory doubles; 21 repository-backed internal tests remain in ordinary and package acceptance.
Existing assertions have been retained, while test implementations and test-only helpers have been removed from production report entries.
The application unit boundary comprises 31 modules and covers all 2,982 lines, 4,474 regions and 348 functions.
Application package coverage is 3,903 of 3,968 lines, or 98.36%.
Versioning package coverage is 382 of 390 lines, or 97.95%; its two unit modules cover all 107 lines, 166 regions and 15 functions from 27 in-memory tests.
The eight collector regression tests, `just check` and `just coverage` pass on macOS arm64.
The coverage reports exclude test implementations and use integer acceptance counts rather than rounded display percentages.
The preceding package figures are therefore not directly comparable with historical reports which included inline test code.
JSON and HTML evidence resides under `coverage/`; execution logs reside in `.validation/coverage-consistency-*.log`.
Linux and other platform-specific execution remain obligations of the corresponding CI jobs.

## Dependency and tooling audit

Version 0.13.3 has been validated on macOS arm64 with Rust 1.99.0 and Python 3.15.0.
The application source digest is `d1e8b455eea720ddbb81508b8153ea290e64d6c43cc5c6c8d794449cc2facd18`, covering the manifest, lockfile, compiler declaration, build script and Rust files under `src/`.
The direct dependency requirements and development-tool pins match their latest stable releases checked on 12 October 2026.
A registry-backed `cargo update --dry-run --verbose` reports no compatible lockfile updates.
Indirect dependencies retain upstream compatibility constraints; this result does not require incompatible major-version substitutions.

Plain `just check` passes with 256 application tests, 41 version-automation tests, separate doctests and release compilation.
Application coverage is 4,876 of 4,950 lines, or 98.51%, with all 3,792 lines, 5,888 regions and 423 functions covered within the strict boundary.
The version helper covers 588 of 596 lines, or 98.66%, with complete line, region and function coverage in its CLI, manifest and policy modules.
The Git library upgrade propagates shallow-boundary errors and disables cache reuse when repository metadata cannot be read reliably.
A regression fixture confirms that malformed shallow metadata cannot produce a successful cached result.

The root package and version helper inherit the same workspace compiler metadata and Rust lint policy.
Setup installs the declared uv and Python versions locally; repeated `sh setup.sh` succeeds without a manual `PATH` override.
Evidence resides in `.validation/dependency-check-final.log`, `.validation/dependency-coverage.log`, `.validation/shallow-metadata-test.log`, `.validation/dependency-setup-repeat.log`, `.validation/dependency-freshness-final.log` and `coverage/`.
Linux and hosted workflow execution remain delegated to CI.
The following sections record earlier application digests.

## Version automation

Version 0.13.3 has been validated on macOS arm64 with Rust 1.99.0.
The application source digest is `d745b2472e25a64dc35d83c285f034b127c46768e73cfcc7c6c2c6265a926de1`, covering the manifest, lockfile, compiler declaration, build script and Rust files under `src/`.
Plain `just check` passes with 255 application tests, 41 version-automation tests, separate doctests and release compilation.
The application retains 98.42% line coverage and complete line, region and function coverage within its strict boundary.
The version helper covers 588 of 596 lines, or 98.66%, with complete line, region and function coverage in its CLI, manifest and policy modules.

An installed `prek` pre-push hook rejects a missing version increment and accepts corrected history against a private local bare remote.
The automated fixtures additionally cover initial commits, amendments, staged lockfile mismatches, unstaged changes, merge rejection, conflicting tags and publication retries.
The release workflow is configured to publish annotated version tags after successful main CI; hosted execution has not been performed locally.
No production tags have been published during acceptance.

Evidence resides in `.validation/version-check-final.log`, `.validation/version-coverage-clean.log`, `.validation/version-installed-hooks.log` and `coverage/`.
An initial coverage attempt included obsolete instrumented objects; the recipe now clears the workspace coverage state before measurement.
Failed attempts remain in the evidence record.
Linux validation remains delegated to the configured CI jobs.

## Earlier engineering baseline

The engineering conventions have been validated on macOS arm64 with Rust 1.99.0 and Python 3.14.8.
The source digest is `8c161d96fe4bed91170da7e876d50fbbef807fb6a304baae6bc7c5505ef38668`; it covers the package manifest, lockfile, compiler declaration, build script and Rust files under `src/`.

Plain `just check` passes with 233 library tests and 22 integration tests, separate doctests and the production release build.
Repository hygiene, Clippy, dependency advisories and policy, unused dependencies, Python linting and Python type checks pass.
Initial and repeated `sh setup.sh` succeed, and plain `prek` requires no repository-specific `PATH` override.
Pre-commit, commit-message and pre-push hooks remain installed; the pre-push hook invokes the full check gate.
The recorded baseline predates the restored version automation.

The package coverage gate records 4,860 of 4,938 lines, or 98.42%.
The strict unit boundary achieves 100% line, region and function coverage, without lowering its thresholds or enlarging its exclusions.
Native configuration tests cover Git Boolean conversion, bare keys, explicit empty values, integer suffixes, includes, case-insensitive keys and command-scope overrides.
An isolated unit test verifies that Git conversion failures remain errors.

The runtime benchmark completes one smoke-test repetition across pipe transport, pseudo-terminal transport and concurrent cache maintenance in a disposable repository.
This run verifies the harness; it does not establish a new performance baseline.
Earlier performance measurements remain identified as historical in [performance](performance.md).

Final evidence resides in `.validation/engineering/git-ls-check-final.log`, `.validation/engineering/git-ls-coverage-complete.log`, the setup and hygiene logs, `.validation/runtime-smoke/runtime-results.json` and `coverage/`.
Failed validation attempts are retained alongside final successes.

CI is configured for Linux and macOS using the same setup and validation recipes.
The current Linux candidate and hosted GitHub Actions have not been executed during this local validation.

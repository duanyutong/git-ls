# Coverage policy

The authoritative acceptance inventory is [coverage-policy.toml](../coverage-policy.toml).
Core Rust unit coverage shall reach 100% of executable lines, regions and functions independently of repository and integration tests.
The Python versioning implementation has a separate 95% line-coverage sanity guard.
This is repository development policy; Cargo does not interpret the manifest.

## Source classification

Every Rust source file under a declared `source_root` shall appear exactly once in the inventory, except test implementation files under `tests/` or named `tests.rs`.
Missing files, obsolete entries, duplicate classifications, empty unit scopes and unknown manifest fields constitute acceptance failures.
The inventory uses complete repository-relative paths, including where several modules share a basename.

| Classification | Obligation |
| --- | --- |
| `unit` | Complete executable line, region and function coverage from the declared unit suite alone. |
| `mixed` | Policy and effects share a module; behavioural verification remains required through ordinary acceptance tests. |
| `boundary` | External execution or resource access; behavioural verification remains required through ordinary acceptance tests. |
| `declarations` | Module declarations, type declarations, constants or re-exports without handwritten executable behaviour. |
| `generated` | Compiler or derive-generated implementation without a handwritten executable coverage entry; explicit behavioural assertions remain required where applicable. |

Classification shall reflect the module's actual responsibilities.
Reclassifying uncovered policy solely to satisfy acceptance is inadmissible.
New or changed policy shall be separated from effects where practicable and included in the unit scope.
Mixed and boundary classifications exclude sources from the unit calculation without removing their behavioural testing obligations.

## Attribution and acceptance

Each suite declares its unit target, optional test filter and optional test-name exclusions through `target`, `test_filter` and `test_skip`.
Unit tests shall exercise production behaviour using values and in-memory doubles, without external commands, repository access, sockets, native credentials or wall-clock waits.
Repository tests shall remain part of `just check`, regardless of their placement within the Rust module tree.
Test implementations shall reside in separate test source files and shall not contribute to reported production metrics.
Coverage exclusions shall be scoped to paths within the checkout, so ancestor directory names cannot conceal production sources.

Each unit suite uses a freshly cleaned profile directory under `target/unit-coverage/<suite>/` and shall not reuse profiles from other test selections.
Unit reports shall contain exactly the declared unit files, with no duplicate or foreign source entries.
Every file shall have positive integer counts and zero uncovered executable lines, regions and functions.
Acceptance uses exact covered and total counts; rounded display percentages cannot establish compliance.
The retained summary identifies the compiler, test selection, source scope and accepted counts.
A failed collection or invalid inventory shall not retain a previous successful summary.

Aggregate application coverage is neither collected nor required by the acceptance workflow.
Branches, MC/DC and generic instantiations are not acceptance metrics.
The selected Rust metrics are all required to reach 100%; there is no supplementary percentage target below 100% for Rust application code.
Coverage runs use the pinned stable compiler, without nightly toolchains, scheduled branch-coverage gates or nightly main CI jobs.
Platform-specific execution remains an obligation of the corresponding CI platform.

## Versioning sanity guard

`just coverage-versioning` measures `scripts/version.py`, including its command subprocesses, and requires at least 95% line coverage.
This guard is independent of the Rust unit metrics and is not part of a combined coverage percentage.
A passing run reports the guard result without displaying its measured percentage; failure displays the detailed report and missing lines.
JSON, HTML and text reports remain available for diagnosis.
No requirement to increase this implementation to 100% follows from the Rust unit policy.

## Commands and evidence

`just test-unit` executes the manifest's declared unit suites without instrumentation.
`just coverage-unit` collects and enforces independent Rust unit coverage.
`just coverage` requires the Rust unit gate and the versioning sanity guard.
`just check` retains the complete behavioural and integration test suite, separate doctests and production release compilation.

Unit JSON, HTML and text reports reside under `coverage/unit/<suite>/`; accepted unit counts and collection metadata reside in `coverage/unit/summary.json`.
Versioning reports reside in `coverage/versioning.json`, `coverage/versioning/` and `coverage/versioning-summary.txt`.
Collector regression checks exercise inventory rejection, basename collisions, duplicate and foreign sources, malformed or incomplete counts, checkout-scoped exclusions, stale-success removal and unit test selection.
These checks form part of `just test`.

Complete line, region and function coverage does not establish complete branch or input-combination coverage, nor does it establish the correctness of assertions.

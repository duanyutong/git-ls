set export
set positional-arguments
RUST_TEST_THREADS := env("RUST_TEST_THREADS", "6")

default:
    @just --list

setup:
    sh setup.sh

fmt:
    prek run cargo-fmt cargo-sort tombi-format ruff-format --all-files

lint:
    uv run scripts/measure.py hygiene prek run --all-files --show-diff-on-failure
    uv run scripts/measure.py semantic prek run pyright cargo-clippy cargo-deny cargo-machete --all-files --hook-stage manual

typecheck:
    cargo check --workspace --all-targets --all-features --locked

test:
    prek run acceptance-tests --all-files --hook-stage manual --verbose
    uv run scripts/measure.py doctests cargo test --workspace --doc --all-features --locked

check:
    just lint
    just test
    just build-release

coverage:
    just coverage-versioning
    prek run coverage-application --all-files --hook-stage manual --verbose
    prek run coverage-unit --all-files --hook-stage manual --verbose

build-release:
    uv run scripts/measure.py release cargo build --release --locked

git_ls_unit_ignore_regex := '(^|/)(src/main\.rs|src/app/(env|cached)\.rs|src/cache(\.rs|/maintenance\.rs)|src/backend/(gix(/.*)?|process)\.rs|src/terminal/detect\.rs|src/test_support\.rs)$'

# Enforce broad package baselines, including binary and process-boundary adapters.
coverage-package-baseline:
    mkdir -p coverage
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --all-targets --all-features --locked --no-report
    cargo llvm-cov report -p git-ls --summary-only --fail-under-lines 95
    cargo llvm-cov report -p git-ls --json --output-path coverage/coverage.json
    cargo llvm-cov report -p git-ls --html --output-dir coverage

# Enforce the strict unit boundary; filesystem, compiled-binary, and process
# adapters are covered by the package gate and integration tests. Pure graph,
# memoisation, configuration parsing, and rendering remain at 100%.
coverage-unit-baseline:
    mkdir -p coverage
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --lib --bins --all-features --locked --no-report
    cargo llvm-cov report -p git-ls --summary-only --fail-uncovered-lines 0 --fail-uncovered-regions 0 --fail-uncovered-functions 0 --ignore-filename-regex '{{git_ls_unit_ignore_regex}}'
    cargo llvm-cov report -p git-ls --json --output-path coverage/unit.json --ignore-filename-regex '{{git_ls_unit_ignore_regex}}'

# Compare with the latest reachable release tag, or include the initial commit.
verify-versions base="0000000000000000000000000000000000000000" head="HEAD":
    cargo run --quiet --locked -p xtask -- version verify-range "$1" "$2"

# Explicit release command; CI invokes this only after successful main validation.
tag-versions base head:
    cargo run --quiet --locked -p xtask -- version tag-range "$1" "$2"

# Preserve the original parent as the version baseline when amending a commit.
amend *args:
    #!/bin/sh
    exec cargo run --quiet --locked -p xtask -- version amend "$@"

coverage-versioning:
    prek run coverage-versioning --all-files --hook-stage manual --verbose

coverage-versioning-baseline:
    mkdir -p coverage
    cargo llvm-cov clean --workspace
    cargo llvm-cov -p xtask --all-targets --locked --no-report
    cargo llvm-cov report -p xtask --summary-only --fail-under-lines 95
    cargo llvm-cov report -p xtask --json --output-path coverage/versioning.json
    cargo llvm-cov report -p xtask --summary-only --fail-uncovered-lines 0 --fail-uncovered-regions 0 --fail-uncovered-functions 0 --ignore-filename-regex '(^|/)xtask/src/(git|main|version)\.rs$'

# git-ls/justfile — common build, test, and lint operations.
#
# Run `just` for the recipe list. Install once with `brew install just` or
# `cargo install just`.

set script-interpreter := ['bash', '-euo', 'pipefail']

git_ls_unit_ignore_regex := '(^|/)(src/main\.rs|src/app/(env|cached)\.rs|src/cache(/.*)?\.rs|src/backend/(gix(/.*)?|process)\.rs|src/terminal/detect\.rs|src/test_support\.rs)$'
xtask_unit_ignore_regex := '(^|/)xtask/src/(git|main|version)\.rs$'

# Show every recipe with its docstring.
default:
    @just --list

# Compile without linking the release binary.
typecheck:
    cargo check --workspace --all-targets --all-features --locked

# Run every test target.
test:
    cargo test --workspace --all-targets --all-features --locked

# Generate package and unit-test coverage, then enforce their separate baselines.
coverage:
    just coverage-package-baseline
    just coverage-unit-baseline

# Enforce broad package baselines, including binary and process-boundary adapters.
coverage-package-baseline:
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --all-targets --all-features --locked --no-report
    cargo llvm-cov report -p git-ls --summary-only --fail-under-lines 95
    cargo llvm-cov report -p xtask --summary-only --fail-under-lines 95

# Enforce the strict unit boundary; filesystem, compiled-binary, and process
# adapters are covered by the package gate and integration tests. Pure graph,
# memoisation, configuration parsing, and rendering remain at 100%.
coverage-unit-baseline:
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --lib --bins --all-features --locked --no-report
    cargo llvm-cov report -p git-ls --summary-only --fail-uncovered-lines 0 --fail-uncovered-regions 0 --fail-uncovered-functions 0 --ignore-filename-regex '{{git_ls_unit_ignore_regex}}'
    cargo llvm-cov report -p xtask --summary-only --fail-uncovered-lines 0 --fail-uncovered-regions 0 --fail-uncovered-functions 0 --ignore-filename-regex '{{xtask_unit_ignore_regex}}'

# Build the optimised binary.
build-release:
    cargo build --release --locked --package git-ls

# Run every lint hook through prek.
lint:
    prek run --all-files --show-diff-on-failure

# Verify commit-level Cargo version policy over a Git range.
verify-versions base="v0.2.1" head="HEAD":
    cargo run --locked -p xtask -- version verify-range "{{base}}" "{{head}}"

# Verify and tag commit-level Cargo versions over a Git range.
tag-versions base head:
    cargo run --locked -p xtask -- version tag-range "{{base}}" "{{head}}"

# Full validation gate.
check:
    just lint
    just typecheck
    just test
    just build-release

# Install development tools and Git hooks.
setup: install-tools install-hooks

# Install project Git hooks.
install-hooks:
    prek install --hook-type pre-commit --hook-type commit-msg --hook-type pre-push

# Install the current development tools used by validation recipes.
install-tools:
    cargo binstall --no-confirm \
        cargo-binstall@1.25.1 \
        cargo-deny@0.20.2 \
        cargo-edit@0.13.13 \
        cargo-llvm-cov@0.9.1 \
        cargo-machete@0.9.2 \
        just@1.58.0 \
        prek@0.5.4 \
        typos-cli@1.50.3
    # The upstream 2.1.4 release asset reports 2.1.3; use the published crate.
    cargo install cargo-sort --version 2.1.4 --locked

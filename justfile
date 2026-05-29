# git-ls/justfile — common build, test, and lint operations.
#
# Run `just` for the recipe list. Install once with `brew install just` or
# `cargo install just`.

set script-interpreter := ['bash', '-euo', 'pipefail']

# Show every recipe with its docstring.
default:
    @just --list

# Compile without linking the release binary.
typecheck:
    cargo check --workspace --all-targets --all-features --locked

# Run every test target.
test:
    cargo test --workspace --all-targets --all-features --locked

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

# Install the current development tools used by `just check`.
install-tools:
    cargo binstall --no-confirm \
        cargo-deny@0.19.8 \
        cargo-machete@0.9.2 \
        cargo-sort@2.1.4 \
        just@1.51.0 \
        prek@0.4.3 \
        typos-cli@1.46.3

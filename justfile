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
    uv run --locked scripts/test_version.py
    uv run scripts/test_coverage.py
    prek run acceptance-tests --all-files --hook-stage manual --verbose
    uv run scripts/measure.py doctests cargo test --workspace --doc --all-features --locked

check:
    just lint
    just test
    just build-release

test-unit:
    uv run scripts/unit_coverage.py --test-only

coverage:
    just coverage-unit
    just coverage-versioning
    prek run coverage-application --all-files --hook-stage manual --verbose

coverage-unit:
    prek run coverage-unit --all-files --hook-stage manual --verbose

build-release:
    uv run scripts/measure.py release cargo build --release --locked

# Compare with the latest reachable release tag, or include the initial commit.
verify-versions base="0000000000000000000000000000000000000000" head="HEAD":
    uv run --locked scripts/version.py verify-range "$1" "$2"

# Explicit release command; CI invokes this only after successful main validation.
tag-versions base head:
    uv run --locked scripts/version.py tag-range "$1" "$2"

# Preserve the original parent as the version baseline when amending a commit.
amend *args:
    #!/bin/sh
    exec uv run --locked scripts/version.py amend "$@"

coverage-versioning:
    uv run --locked coverage erase
    uv run --locked coverage run scripts/test_version.py
    uv run --locked coverage combine
    uv run --locked coverage report --fail-under=95
    uv run --locked coverage json -o coverage/versioning.json
    uv run --locked coverage html -d coverage/versioning

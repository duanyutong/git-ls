# git-ls

`git-ls` renders local `git-branchless` draft branches as compact coloured stack lanes for use as `git ls`.

## Prerequisites

- Git.
- `git-branchless`, initialised in the repository to be inspected.
- Rust. This repository includes `rust-toolchain.toml` for the standard formatting and linting components.

## Build & Install

```sh
cargo +stable install --path . --locked --profile release
```

Cargo builds the local crate with the release profile and installs the binary into its configured binary directory, commonly `~/.cargo/bin`. When that directory is on `PATH`, Git exposes the executable as `git ls`.

## Run

```sh
git ls
git ls --color never
git ls --order oldest
git ls 'draft() & branches(feature/)'
```

```text
--hidden          include hidden commits when evaluating revsets
--order VALUE     order stack lanes by head commit time: newest or oldest
--color VALUE     colour mode: auto, always, or never
```

## Development

```sh
just install-tools
just check
```

## Licence

This project is licensed under the GNU General Public Licence version 3 only.
See [LICENSE](LICENSE).

# git-ls

`git-ls` renders local `git-branchless` draft branches as compact coloured stack lanes for use as `git ls`.

## Prerequisites

- Git.
- `git-branchless`, initialised in the repository to be inspected.
- Rust. This repository includes `rust-toolchain.toml` for the standard formatting and linting components.

## Build & Install

One-line commands to build and install:

```sh
# release build
rustup run stable cargo install --path . --locked

# development build
rustup run stable cargo install --path . --locked --debug
```

Cargo installs the local crate with the release profile by default, placing the binary in its configured binary directory, commonly `~/.cargo/bin`. Use the debug command for rapid local iteration. When that directory is on `PATH`, Git exposes the executable as `git ls`.

## Run

```sh
git ls
git ls --backend shell
git ls --color never
git ls --order oldest
git ls --version
git ls 'draft() & branches(feature/)'
```

```text
--hidden          include hidden commits when evaluating revsets
--backend VALUE   Git plumbing backend: gix or shell
--order VALUE     order stack lanes by head commit time: newest or oldest
--color VALUE     colour mode: auto, always, or never
```

## Development

```sh
just install-tools
just install-hooks
just check
```

## Licence

This project is licensed under the GNU General Public Licence version 3 only.
See [LICENSE](LICENSE).

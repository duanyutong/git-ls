# git-ls

`git-ls` renders local `git-branchless` branches as compact coloured stack lanes for use as `git ls`.

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

Cargo installs the local crate with the release profile by default, placing the binary in its configured binary directory, commonly `~/.cargo/bin`.
When that directory is on `PATH`, Git exposes the executable as `git ls`.

If Cargo shows curl timeout errors, use `--frozen` instead of `--locked` to avoid attempting to update the lockfile.

## Usage

```sh
git ls
git ls --version
git ls --backend shell
git ls --color never
git ls --palette okabe
git ls --order oldest
git ls -v
git ls -vv
git ls 'draft() & branches(feature/)'
```

```text
--hidden          include hidden commits when evaluating revsets
-v, --verbose     increase branch annotation verbosity; repeat as -vv for commit titles
--backend VALUE   Git plumbing backend: gix or shell
--order VALUE     order stack lanes by head commit time: newest or oldest
--color VALUE     colour mode: auto, always, or never
--palette, -p VALUE
                  lane colour palette: okabe, tableau, dark2, set1, set2,
                  paired, bold, vivid, tol, or classic
```

## Configuration

`git-ls` reads Git configuration from the ordinary Git configuration stack.
Local repository configuration overrides global configuration, and explicit command-line options override both.

```ini
[git-ls]
  backend = gix
  palette = classic
  verbosity = 2
```

## Glyph System

The graph uses `git-branchless` glyphs for equivalent concepts, preserving a branch-oriented layout while aligning commit vocabulary with `git branchless smartlog`.

| Symbol | Meaning |
|---|---|
| `●` | Current branch head. |
| `◯` | Non-current branch head or branch point. |
| `◇` | Main-history node. |
| `◆` | Current main-history node. |
| `⦸` | Branch group orphaned from main history; rendered in a separate grey warning lane, labelled `(orphaned)`, and excluded from lane palette rotation. |
| `▶` | Current branch row indicator. `git-branchless` uses `ᐅ`; `git-ls` uses `▶` in a leading gutter so the graph glyph remains adjacent to the branch name. |
| `│` | Visible main or stack ancestry. |
| `⁝` | Omitted or elided main-history continuation. A counted form, such as `⁝ (531 commits on main)`, represents a collapsed main-history segment. |
| `──` | Empty main-history connection stub, used when no stack lane is attached to the shown main node. |
| `─┴` | Intermediate connection from main into a stack lane. |
| `─┘` | Final connection from main into a stack lane. |

## Development

```sh
just setup
just check
```

# git-ls

`git-ls` renders local Git branch stacks as compact coloured lanes for use as `git ls`.

## Prerequisites

- Git.
- `git-branchless` is optional. The default `git ls` command uses plain Git, selecting unmerged local branches and inferring stack heads from ancestry. Custom branchless revsets, and default selections made with `--hidden`, require `git-branchless`.
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
git ls -h
```

## Configuration

`git-ls` reads Git configuration from the ordinary Git configuration stack.
Local repository configuration overrides global configuration, and explicit command-line options override both.

```ini
[git-ls]
  backend = gix
  layout = columns
  palette = classic
  verbosity = 2
```

## Plain Git Selection

The default command resolves the main branch from `branchless.core.mainBranch`, `main`, `master`, or `trunk`, in that order. It then selects local branches not already contained in main, identifies stack heads by ancestry, and renders them through the same graph, metadata, ordering, and colour pipeline.

Custom branchless revsets are not interpreted by the plain-Git selector. A command such as `git ls 'draft() & branches(feature/)'` therefore requires `git-branchless`, because plain Git has no equivalent syntax for `draft()`, `public()`, or `heads(...)`. The same branchless-backed selector is used when the default revset is combined with `--hidden`, because hidden commit visibility is a branchless-specific concept.

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

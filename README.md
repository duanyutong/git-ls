# git-ls

`git-ls` renders local Git branch stacks as compact coloured lanes for use as `git ls`.

![Example stack display](docs/assets/git-ls-example.svg)

## Glyph System

The graph uses `git-branchless` glyphs for equivalent concepts, with branch heads arranged above their ancestors.
Stacks with a common downstack path share that path and connect through a junction above the shared branch point.
Colours rotate in display order: each uninterrupted stack segment retains its colour, and a shared downstack segment receives the next colour after its child stacks join.
The rotation continues across stack groups and repeats when the palette is exhausted.
The following example uses synthetic branch names:

```text
    ◯   feature/search
    │ ◯ feature/export
    ├─┘
▶   ●   feature/api
  ⁝ ◯   feature/base
  ◇─┘   main
```

| Symbol | Meaning |
| --- | --- |
| `●` | Current branch head. |
| `◯` | Non-current branch head or branch point. |
| `◉` | Branch point at `HEAD` when it is not the current named branch, including a detached `HEAD`. |
| `◇` | Main-history node. |
| `◆` | Current main-history node. |
| `⦸` | Branch group orphaned from main history, labelled `(orphaned)` and rendered in a separate grey lane excluded from palette rotation. |
| `✕` | Rewritten commit, accompanied by its replacement commit. |
| `▶` | Current branch row indicator in the leading gutter. |
| `│` | Visible main or stack ancestry. |
| `⁝` | Omitted main-history continuation; a counted form, such as `⁝ (12 commits on main)`, denotes a collapsed segment. |
| `──` | Horizontal ancestry connection, or an empty connection stub beside a main-history node. |
| `├` | Junction joining child stacks to their shared downstack path. |
| `─┴` | Intermediate connection into a stack lane. |
| `─┘` | Final connection into a stack lane. |

## Integration Support

Supported VCS backends are:

- Git, optionally using [git-branchless](https://github.com/arxanas/git-branchless) for custom revsets and hidden-commit selection.

Git is required for both the native `gix` backend and the Git subprocess backend.

## Installation

Source installation requires Rust.

```sh
cargo install --path . --locked
```

Installation details and display examples are documented in [common commands](docs/commands.md).

## Configuration

Settings reside in the `[git-ls]` section of Git configuration.
Example `~/.gitconfig` section:

```gitconfig
[git-ls]
    backend = gix
    layout = columns
    palette = classic
    verbosity = 2
    cache = true
    cacheMaintenance = true
```

Option definitions and precedence are documented in [configuration](docs/configuration.md).
Verbosity `2` includes per-layer added and deleted line counts; `--diff-stat`, `--no-diff-stat` and `git-ls.diffStat` override that preset.
Development instructions appear in [development](docs/development.md).

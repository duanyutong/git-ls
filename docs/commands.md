# Common commands

## Installation

Source installation requires the compiler specified in `rust-toolchain.toml`.
The standard installation uses the optimised release profile:

```sh
cargo install --path . --locked
```

Cargo installs the executable in its configured binary directory, commonly `~/.cargo/bin`.
When that directory is on `PATH`, Git exposes the executable as `git ls`.
The `--debug` option selects a development build.
The `--frozen` option additionally disables network access and therefore requires all dependencies to be cached.

## Display

```sh
git ls                             # Local branch stacks.
git ls --layout columns            # Column layout.
git ls --backend shell             # Git subprocess backend.
git ls --no-cache                  # Fresh analysis.
git ls --debug                     # Progress and timings on standard error.
git ls --help                      # Complete command reference.
```

## Configuration

```sh
git config --global git-ls.layout columns
git config --local git-ls.layout inline
git config --show-origin --get-regexp '^git-ls\.'
```

## Plain Git Selection

The default command resolves the main branch from `branchless.core.mainBranch`, `main`, `master`, or `trunk`, in that order.
It then selects local branches not already contained in main, identifies stack heads by ancestry, and renders them through the same graph, metadata, ordering, and colour pipeline.

Custom branchless revsets are not interpreted by the plain-Git selector.
A command such as `git ls 'draft() & branches(feature/)'` therefore requires `git-branchless`, because plain Git has no equivalent syntax for `draft()`, `public()`, or `heads(...)`.
The same branchless-backed selector is used when the default revset is combined with `--hidden`, because hidden commit visibility is a branchless-specific concept.

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

## Performance Diagnostics

`git ls --debug` reports contextual progress and performance records on standard error, leaving the rendered graph on standard output unchanged.
Records include configuration and repository discovery, selection, lane and main-history traversal, formatting, output, and observable backend-operation counts.

Durations use a monotonic clock and milliseconds.
Stage and operation totals are inclusive: nested durations must not be added to their enclosing stages.
`observed_total_ms` starts after argument parsing and ends before summary emission; it excludes process launch and earlier terminal detection.
Debug output itself can affect elapsed time.
Without `--debug`, diagnostic clocks, counters, and report allocations are disabled.

The [implementation and evaluation report](performance.md) records the algorithms, cache validity rules, measured costs, and reproducible benchmark commands.

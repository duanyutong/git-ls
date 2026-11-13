# Common commands

## Installation

Source installation requires the compiler specified in `rust-toolchain.toml`.
Displaying the manual requires `man`.
The standard installation uses the optimised release profile and installs the executable and its manual under `~/.cargo`:

```sh
./install.sh
```

An optional prefix may be supplied as `./install.sh /absolute/prefix`.
The installer always passes the default or supplied prefix to Cargo as `--root`.
This deliberate installer policy overrides `CARGO_INSTALL_ROOT`, Cargo's `install.root` configuration and `CARGO_HOME` for the installation destination, in accordance with [Cargo's installation-root precedence](https://doc.rust-lang.org/cargo/commands/cargo-install.html#description).
To retain a configured installation destination, supply that directory explicitly.
The helper does not infer Cargo's configured destination.

The prefix's `bin` directory must be on `PATH` so that Git can locate the executable and the manual viewer can discover the adjacent `share/man` directory.
If `MANPATH` is explicitly set, include the prefix's `share/man` directory whilst retaining the existing manual search paths.
Direct installation with `cargo install --path . --locked` installs only the executable.
For direct Cargo installation, `--debug` selects a development build and `--frozen` additionally disables network access, requiring all dependencies to be cached.

Use the helper for subsequent source updates to keep the executable and manual aligned.
Cargo does not update or remove the separately installed manual; after `cargo uninstall git-ls`, remove `<prefix>/share/man/man1/git-ls.1` separately.
Package maintainers may generate the same manual from a compiled executable with `./scripts/man.sh ./target/release/git-ls > git-ls.1`.

## Display

```sh
git ls                             # Local branch stacks.
git ls --layout columns            # Column layout.
git ls --backend shell             # Git subprocess backend.
git ls --no-cache                  # Fresh analysis.
git ls --debug                     # Progress and timings on standard error.
git ls -vv                         # Detailed display, including layer diff statistics.
git ls -vv --no-diff-stat           # Detailed display with line counts suppressed.
git ls -v --diff-stat               # Normal display with line counts requested.
git ls --help                      # Complete command reference.
```

Git resolves `git ls --help` and `git help ls` through its manual viewer.
The commands `git ls -h` and `git-ls --help` display the built-in help without requiring an installed manual or a manual viewer.

## Configuration

```sh
git config --global git-ls.layout columns
git config --local git-ls.layout inline
git config --global git-ls.verbosity 2
git config --global git-ls.diffStat false
git config --show-origin --get-regexp '^git-ls\.'
```

Verbosity selects the display preset; explicit diff-stat flags override `git-ls.diffStat`, which otherwise overrides the verbosity preset.
When `git-ls.diffStat` is unset, verbosity `2` includes counts and levels `0` and `1` omit them.
The [configuration reference](configuration.md#display-detail-and-diff-statistics) specifies the complete precedence and measurement policy.

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
| `⦸` | Branch group orphaned from main history; rendered in a separate grey warning lane, labelled `(orphaned)`, and excluded from stack colour assignment. |
| `▶` | Current branch row indicator. `git-branchless` uses `ᐅ`; `git-ls` uses `▶` in a leading gutter so the graph glyph remains adjacent to the branch name. |
| `│` | Visible main or stack ancestry. |
| `⁝` | Omitted or elided main-history continuation. A counted form, such as `⁝ (531 commits on main)`, represents a collapsed main-history segment. |
| `──` | Empty main-history connection stub, used when no stack lane is attached to the shown main node. |
| `─┴` | Intermediate connection from main into a stack lane. |
| `─┘` | Final connection from main into a stack lane. |

## Performance Diagnostics

`git ls --debug` reports contextual progress and performance records on standard error, leaving the rendered graph on standard output unchanged.
Records include configuration and repository discovery, selection, lane and main-history traversal, formatting, output, and observable backend-operation counts.
They also identify graph-interpretation delegation, rejected cache publications, retained metadata counts and pending maintenance work.

Durations use a monotonic clock and milliseconds.
Stage and operation totals are inclusive: nested durations must not be added to their enclosing stages.
`observed_total_ms` starts after argument parsing and ends before summary emission; it excludes process launch and earlier terminal detection.
Debug output itself can affect elapsed time.
Without `--debug`, diagnostic clocks, counters, and report allocations are disabled.

The [implementation and evaluation report](performance.md) records the algorithms, cache validity rules, measured costs, and reproducible benchmark commands.

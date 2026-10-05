# git-ls

`git-ls` renders local Git branch stacks as compact coloured lanes for use as `git ls`.

![Example `git ls` output showing generic branch stacks](docs/assets/git-ls-example.svg)

## Prerequisites

- Git.
- `git-branchless` is optional. The default `git ls` command uses plain Git, selecting unmerged local branches and inferring stack heads from ancestry. Custom branchless revsets, and default selections made with `--hidden`, require `git-branchless`.
- Rust 1.99.0. The repository pins the compiler and required formatting, linting, and coverage components in `rust-toolchain.toml`.

## Build & Install

One-line commands to build and install:

```sh
# release build
cargo install --path . --locked

# development build
cargo install --path . --locked --debug
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

## Performance Diagnostics

`git ls --debug` reports contextual progress and performance records on standard error, leaving the rendered graph on standard output unchanged. Records include configuration and repository discovery, selection, lane and main-history traversal, formatting, output, and observable backend-operation counts.

Durations use a monotonic clock and milliseconds. Stage and operation totals are inclusive: nested durations must not be added to their enclosing stages. `observed_total_ms` starts after argument parsing and ends before summary emission; it excludes process launch and earlier terminal detection. Debug output itself can affect elapsed time. Without `--debug`, diagnostic clocks, counters, and report allocations are disabled.

The [implementation and evaluation report](docs/performance.md) records the algorithms, cache validity rules, measured costs, and reproducible benchmark commands.

## Persistent analysis reuse

By default, ordinary `draft()` selections reuse analysis stored in
`<git-common-dir>/git-ls/cache/v1/`. In a conventional checkout, this is
`.git/git-ls/cache/v1/`; linked worktrees share the common repository cache.
The cache is disposable and is never committed.

An unchanged reference snapshot reuses neutral topology and commit metadata.
Changed branches invalidate that snapshot, whilst successful merge-base,
main-distance, and metadata results remain reusable under their full object-ID
keys. At publication, each fact category retains up to 512 recently used entries.
Branch names and the resolved main configuration participate in validation.
Worktree markers, ages, ordering, verbosity, layout, and colour are applied afresh.

- `--no-cache` bypasses persistent reads and writes.
- `--refresh-cache` recomputes the analysis and publishes a successful replacement.
- `--clear-cache` removes this version's application cache and renders uncached.
- `--debug` reports eligibility, hits, validation, loading, publication, and reuse
  counters. Cache hits do not report earlier computation timings as current work.

Storage uses 32 snapshot slots, two backend-specific fact files, and one temporary
publication, each limited to 1 MiB. Consequently, admitted data occupies at most
35 MiB, independently of maintenance. Slot collisions cause replacement or a
cache miss; the complete stored key is always validated. Atomic publication and
non-blocking locks permit concurrent worktrees. Corrupt, incompatible, oversized,
busy, or unavailable entries fall back to computation. Configuration or references
which change during computation prevent publication.

Persistent reuse is bypassed for branchless selections, hidden selections,
shallow repositories, replacements, grafts, alternate object stores, partial
clones, and unsupported graph-related environment overrides. These modes continue
to use the optimised uncached algorithms. The native backend retains at most
65,536 application parent records per invocation; this limit is separate from
Git's or gix's internal object caches.

## Deferred cache maintenance

After output has been written and flushed, eligible cache publications contribute
to repository-wide maintenance counters. A worker may be scheduled after sixteen
publications or 4 MiB of admitted snapshot data, with at least one hour between
attempts. `--no-cache-maintenance`
suppresses scheduling. `--no-cache` also suppresses all maintenance activity.

The short-lived worker has null standard streams and a separate process group;
on Unix it requests reduced priority through `nice`. It acquires non-blocking
locks, examines at most 32 snapshot slots, and applies a 100 ms cooperative time
budget and an 8 MiB reclamation budget. Filesystem calls themselves cannot be
interrupted by that time budget. It preferentially retains eight recent snapshots
and the triggering snapshot; entries older than seven days are eligible for
removal. Reusable fact files remain bounded independently. This mechanism never
runs `git gc`, traverses repository history, or accesses the network.

`--debug` distinguishes a scheduled attempt from the previous worker's completed
report. Failed spawning, contention, interruption, or disabled maintenance leaves
the foreground result valid. The fixed-slot storage limit remains effective in
all these cases; maintenance adds only bounded state and report records.

Repository or user configuration may disable reuse with `git config git-ls.cache
false`, or disable deferred maintenance with `git config git-ls.cacheMaintenance
false`. An explicit `--refresh-cache` overrides the former setting for that
invocation. Configuration is retrieved in one Git subprocess, preserving Git's
include processing and precedence; malformed configuration remains an error.

## Development

```sh
just setup
just check
```

## Licence

This project is licensed under the GNU Affero General Public License, version 3 only (`AGPL-3.0-only`). See [LICENSE](LICENSE).

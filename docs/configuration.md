# Configuration

Persistent settings reside in the `[git-ls]` section of Git configuration.
Git handles file parsing, includes, scope precedence and Boolean conversion.
Configuration keys use camelCase; command-line flags use kebab-case.
Explicit command-line options override the resolved configuration.

| Key | Meaning and default |
| --- | --- |
| `backend` | Repository backend: `gix` or `shell`; `gix`. |
| `layout` | Graph layout: `inline` or `columns`; `inline`. |
| `palette` | Colour palette; `classic`. |
| `verbosity` | Detail level from `0` to `2`; `1`. |
| `diffStat` | Added and deleted line counts per stack layer; follows verbosity when unset. |
| `cache` | Persistent analysis reuse; `true`. |
| `cacheMaintenance` | Deferred cache maintenance; `true`. |

The [Git configuration rules](https://git-scm.com/docs/git-config) determine precedence: command configuration overrides worktree, repository, user and system configuration, in that order.
Worktree configuration applies when `extensions.worktreeConfig` is enabled.
The last value for each supported key wins.
Empty display settings use their application defaults; a bare Boolean key means true, and an explicitly empty Boolean value means false.
Git performs Boolean conversion, including integer forms; invalid Boolean values and malformed configuration produce errors.

## Display detail and diff statistics

Verbosity selects a display preset; the diff-stat setting overrides only the inclusion of line counts.
Level `0` shows the graph, branch names and current-branch markers.
Level `1`, the default, additionally shows commit ages and layer commit counts.
Level `2` additionally shows commit subjects, abbreviated object identifiers and diff statistics.
The options `-v` and `-vv` select levels `1` and `2`, respectively; further repetitions retain level `2`.
An explicit verbosity option overrides configured verbosity rather than incrementing it.
The independent `--debug` flag enables diagnostic output on standard error.

Diff statistics follow this precedence:

1. An explicit `--diff-stat` or `--no-diff-stat` option determines whether counts are requested.
2. Otherwise, the resolved Boolean value of `git-ls.diffStat` applies when present.
3. Otherwise, counts are requested at verbosity `2` and omitted at levels `0` and `1`.

The two opposing CLI options conflict and cannot be supplied together.
Changing verbosity does not reset an explicit diff-stat preference.
An unset configuration value follows verbosity; `false` always suppresses counts unless `--diff-stat` overrides it.
Git configuration scope precedence applies to `diffStat` in the same manner as other persistent settings.

Each `+added -deleted` pair describes the combined tree difference between the preceding branch point and the layer head.
The first layer is compared with its main-history base; an unrelated history is compared with the empty tree.
Intermediate edits within a layer are not accumulated separately.
Counts include changed text files, recognise renames, and exclude binary line counts.
External diff commands and text-conversion drivers are disabled.
Both repository backends delegate the diff calculation to Git, preserving its attribute-based text and binary classification.
Diffs are calculated only when requested and are recalculated for each invocation, including when topology is obtained from the cache.

Additions use the terminal's standard ANSI green and deletions use standard ANSI red, independently of the graph palette.
The signs and alignment are retained when colour is disabled.
The columns layout places counts immediately before the abbreviated object identifier; the inline layout appends counts while retaining its existing metadata prefix.
Main-history and rewritten-commit rows carry no layer statistics.
Where the terminal cannot accommodate the complete branch labels and statistic columns, statistics are omitted consistently from every row before branch labels are truncated.

## Persistent analysis reuse

By default, ordinary `draft()` selections reuse analysis stored in `<git-common-dir>/git-ls/cache/v1/`.
In a conventional checkout, this is `.git/git-ls/cache/v1/`; linked worktrees share the common repository cache.
The cache is disposable and is never committed.

An unchanged reference snapshot reuses neutral topology, whilst optional commit metadata is obtained separately for the requested verbosity.
Changed branches invalidate that snapshot, whilst successful merge-base, main-distance, and metadata results remain reusable under their full object-ID keys.
At publication, each fact category retains up to 512 recently used entries.
Byte admission reduces optional metadata before graph facts, preserving reusable topology independently of oversized commit subjects.
Snapshot construction does not increase the requested verbosity.
Branch names and the resolved main configuration participate in validation.
Worktree markers, ages, ordering, verbosity, layout, and colour are applied afresh.

- `--no-cache` bypasses persistent reads and writes.
- `--refresh-cache` recomputes the analysis and publishes a successful replacement.
- `--clear-cache` removes this version's application cache and renders uncached.
- `--debug` reports eligibility, hits, validation, loading, publication, and reuse counters.
  Cache hits do not report earlier computation timings as current work.

Storage uses 32 snapshot slots, two backend-specific fact files, and one temporary publication, each limited to 1 MiB.
Consequently, admitted data occupies at most 35 MiB, independently of maintenance.
Slot collisions cause replacement or a cache miss; the complete stored key is always validated.
Atomic publication and non-blocking locks permit concurrent worktrees.
Corrupt, incompatible, oversized, busy, or unavailable entries fall back to computation.
Configuration or references which change during computation prevent publication.

Persistent reuse is bypassed for branchless selections, hidden selections, shallow repositories, replacements, grafts, alternate object stores, partial clones, and unsupported graph-related environment overrides.
These modes continue to use the optimised uncached algorithms.
When replacement refs, grafts or their environment overrides are present, native selection delegates to the Git subprocess backend to preserve Git's graph interpretation.
Debug output identifies the effective backend.
The native backend retains at most 65,536 application parent records per invocation; this limit is separate from Git's or gix's internal object caches.

## Deferred cache maintenance

After output has been written and flushed, eligible cache publications contribute to repository-wide maintenance counters.
A worker may be scheduled after sixteen publications or 4 MiB of admitted snapshot data, with at least one hour between attempts.
`--no-cache-maintenance` suppresses scheduling.
`--no-cache` also suppresses all maintenance activity.

The short-lived worker has null standard streams and a separate process group; on Unix it requests reduced priority through `nice`.
It acquires non-blocking locks, examines at most 32 snapshot slots, and applies a 100 ms cooperative time budget and an 8 MiB reclamation budget.
Filesystem calls themselves cannot be interrupted by that time budget.
It preferentially retains eight recent snapshots and the triggering snapshot; entries older than seven days are eligible for removal.
Retained entries do not consume the reclamation budget, and oversized entries cannot block eligible smaller entries.
Deferred reclamation remains eligible after the minimum interval without requiring additional publications.
Reusable fact files remain bounded independently.
This mechanism never runs `git gc`, traverses repository history, or accesses the network.

`--debug` distinguishes a scheduled attempt from the previous worker's report, including whether reclamation remains pending.
Failed spawning, contention, interruption, or disabled maintenance leaves the foreground result valid.
The fixed-slot storage limit remains effective in all these cases; maintenance adds only bounded state and report records.

Repository or user configuration may disable reuse with `git config git-ls.cache false`, or disable deferred maintenance with `git config git-ls.cacheMaintenance false`.
An explicit `--refresh-cache` overrides the former setting for that invocation.
Configuration discovery uses one Git subprocess; configured Boolean keys receive typed Git lookups for their effective values.

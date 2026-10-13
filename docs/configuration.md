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
| `cache` | Persistent analysis reuse; `true`. |
| `cacheMaintenance` | Deferred cache maintenance; `true`. |

The [Git configuration rules](https://git-scm.com/docs/git-config) determine precedence: command configuration overrides worktree, repository, user and system configuration, in that order.
Worktree configuration applies when `extensions.worktreeConfig` is enabled.
The last value for each supported key wins.
Empty display settings use their application defaults; a bare Boolean key means true, and an explicitly empty Boolean value means false.
Git performs Boolean conversion, including integer forms; invalid Boolean values and malformed configuration produce errors.

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

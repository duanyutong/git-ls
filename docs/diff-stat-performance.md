# Per-layer statistics: performance and reuse

**Date:** 26 October 2026.
**Baseline:** `0fac0c27606d4cb9219f40e31edbd3ae7ce6bb19`, version `0.15.0`.

## Measured comparison

The local measurement evidence under `.agents/plans/git-ls-performance-20261026/` contains every timing sample, binary identity, effective fixture configuration, output digest, diagnostic counter, and Git process-family count.
The maintained harness regenerates these data in a caller-specified directory.
The experiment used the native backend on macOS 27.0.1, Apple Silicon, Git 2.56.0, and Rust 1.99.1.
Each table entry is the median elapsed duration, in milliseconds, of nine fresh processes; filesystem caches remained warm, and timing order was reproducibly shuffled.
Cold samples remove the application cache before each invocation, whereas warm samples reuse populated entries.
The original and revised binaries received equivalent isolated repositories, and every compared mode and terminal probe produced identical output.

| Fixture | Original warm | Revised warm | Original cold | Revised cold |
| --- | ---: | ---: | ---: | ---: |
| Independent layer, 1 | 50.36 | 44.07 | 54.59 | 113.02 |
| Independent layers, 8 | 167.12 | 49.68 | 177.23 | 131.30 |
| Independent layers, 32 | 610.62 | 48.17 | 619.62 | 148.48 |
| Independent layers, 128 | 2426.58 | 61.30 | 2476.83 | 180.58 |
| Linear stack, 32 | 672.96 | 49.51 | 670.43 | 156.40 |
| Shared trunk, 16 + 32 | 957.06 | 61.81 | 1045.41 | 170.50 |
| Equivalent endpoint trees, 32 | 628.62 | 31.98 | 631.98 | 60.38 |
| Orphan roots, 8 | 324.94 | 50.07 | 327.02 | 159.04 |
| Edited renames, 200 files | 60.15 | 54.60 | 52.95 | 128.84 |
| Edited renames, 1,000 files | 99.75 | 55.49 | 98.24 | 218.24 |
| Text file, 200,000 initial lines | 69.50 | 69.82 | 80.12 | 176.13 |
| Binary file, 1 | 45.16 | 47.12 | 50.50 | 121.31 |
| Metadata, 600 branches; statistics disabled | 120.59 | 86.51 | — | — |

For 128 independent layers, warm execution is approximately 39.6 times faster in this sample.
With persistence disabled, the same fixture changes from 2534.50 ms to 62.25 ms.
Its original warm trace contains 128 diff processes; the revised warm trace contains one configuration query and one attribute query, with no diff process.
The revised uncached trace contains one diff batch; initial cache population contains two batches for dependency discovery and validation.
The narrow-terminal trace contains neither a diff nor an attribute query.
The equal-tree fixture launches no diff process even without persistence, and eight orphan roots require only one empty-tree resolution per uncached invocation.

The 600-branch metadata probe records one repeated facts-entry write in the original binary and none in the revised binary.
Statistics-disabled timings otherwise vary in both directions: the one-layer fixture changes from 33.31 ms to 27.13 ms, whilst the 128-layer fixture changes from 39.79 ms to 43.88 ms and the linear stack changes from 28.89 ms to 38.96 ms.
These measurements do not establish a uniform improvement for selections which do not calculate statistics; bounded statistics facts also enlarge the retained facts entry.

Single-layer text and binary workloads show little warm improvement, and initial population costs more than the original single comparison.
The 1,000-file edited-rename case, for example, improves from 99.75 ms to 55.49 ms when warm but changes from 98.24 ms to 218.24 ms when cold.
These costs follow from the conservative validity contract described below and must be considered separately from the elimination of per-layer process growth.
The values are evidence for this host and fixture set, rather than latency guarantees.

## Acquisition and presentation

Statistics continue to describe the net difference between each layer's endpoint trees.
The verbosity-2 preset and the existing Boolean configuration and command-line overrides retain their established meanings.
Rename detection, binary-file handling, disabled external diff commands, and disabled text conversion remain unchanged.

The application collects distinct directional commit pairs across all lanes before requesting statistics.
The native backend resolves endpoint trees through its object database; the shell backend resolves all endpoints through one `cat-file --batch-check` request.
Equal endpoint trees produce zero counts without a diff request, and distinct commit pairs with identical ordered tree pairs share one comparison.
Remaining comparisons use one `diff-tree --stdin` process per batch, independent of the number of layers.
Tree-pair headers frame NUL-delimited numstat records, including rename records, empty results, and filenames containing tabs or newlines.
The parser validates framing, object formats, numeric conversion, and addition overflow.
Orphan roots share one repository-format-specific empty-tree resolution per invocation, using null standard input without a temporary spool file.
SHA-256 repositories retain the shell backend's existing support.

An explicit terminal-width preflight renders minimum-width placeholder counts before hydration.
If even those counts cannot fit under the existing all-or-nothing visibility rule, the application issues no statistics request.
The final width check remains necessary because actual counts may require more digits.
Piped output retains statistics because its width is unknown.
Decimal-width discovery does not allocate strings, and hiding statistics restores the original row without retaining a boxed duplicate of the complete line.

## Persistent statistics

Eligible ordinary selections persist successful directional endpoint-pair counts alongside other bounded facts.
Topology snapshots remain independent of optional display statistics.
An unchanged warm invocation validates the diff context and reuses the counts without executing a diff process or resolving endpoint trees.
New endpoint pairs extend the retained working set whilst existing valid pairs remain reusable.

The versioned context covers the canonical worktree root, effective Git configuration, the resolved Git executable's canonical location and file metadata, and Git's effective attributes for the union of compared paths.
Git's `check-attr --all -z --stdin` resolves worktree, index, repository-info, global, and system attribute precedence.
Both names of a renamed path participate in validation.
Linked worktrees therefore share graph facts without sharing attribute-sensitive counts indiscriminately.
Contexts which cannot be established conservatively bypass statistics persistence, including paths which cannot be represented losslessly by the current command adapter.
Existing graph-interpretation and repository eligibility restrictions also remain applicable.

On a cold statistics miss, the first batch discovers affected paths and a second batch measures counts between context captures.
This deliberately adds validation work to initial cache population, whilst avoiding dependence on endpoint identifiers alone.
Changed contexts or newly discovered dependencies prevent publication.
If a context changes whilst a partial hit is being extended, the application recomputes the complete requested batch rather than combining stale hits with fresh results.
Failed or incomplete batches are never persisted.

Each facts category retains at most 4,096 entries at publication, subject to the existing 1 MiB encoded-entry bound.
Byte admission discards optional statistics before metadata and graph facts.
The independent storage bound remains unchanged.
Publication is skipped when admission would reproduce the existing answers, preventing repeated writes and reference revalidation for an unchanged retained working set.
The larger bounded working set also accommodates the previously observed 600-branch metadata case.

Configuration discovery captures effective values in one Git call and collectively converts configured Boolean settings in at most one additional typed Git call.
Git continues to perform parsing, include resolution, precedence, Boolean conversion, and invalid-value validation.
Valid UTF-8 subprocess output transfers its owned buffer into the parser without a second complete copy.
NUL-delimited command input and output retain their framing.

## Diagnostics and regression coverage

The `diff_stats` stage includes acquisition and persistence decisions.
Counters distinguish requested commit pairs, distinct commit pairs, distinct tree pairs, equal-tree skips, batch launches, and persistent hits and misses.
Diff, attribute, empty-tree, metadata, and configuration subprocesses have separate diagnostic families.
Bypass and terminal-suppression events identify why reusable or visible statistics were unavailable.

Unit tests exercise batch framing, malformed and overflowing records, directional deduplication, bounded persistence, partial hits, changing dependencies, and both rendering layouts.
Integration tests exercise actual Git across both backends, including merges, equivalent trees at distinct commits, reverted changes within a layer, aliases, edited and unusual rename paths, binary files, orphan roots, nested invocation directories, SHA-256 identifiers, attribute sources, linked worktrees, and configuration changes.
Subprocess-count assertions provide deterministic regression checks without imposing host-dependent millisecond thresholds.

## Acceptance validation

The isolated implementation passes `just check`, including 316 Rust tests, and `just coverage`.
Required Rust unit coverage is 100 per cent for lines, regions, and functions across 35 modules; the separate versioning coverage guard also passes its 95 per cent minimum.
The maintained benchmark verifies output parity independently of the functional tests and does not impose host-dependent timing thresholds.

## Reproduction and residual costs

The maintained harness creates disposable repositories and isolates user and system Git configuration.
Candidate and baseline binaries use separate application-cache namespaces with equivalent fixture contents.
Timing order is reproducibly shuffled; raw wall-clock, child CPU, and peak resident-memory samples accompany binary identities, diagnostics, Git process traces, and output-parity checks.
Cases include enabled and disabled statistics, explicit low-verbosity statistics, cold application caches, refreshes, warm reuse, increasing layer counts, stacks, shared ancestry, equal trees, orphan roots, edited renames, large text, binary files, metadata retention, and narrow pseudo-terminals where supported.

```sh
uv run scripts/benchmark-diff-stat.py \
  --candidate target/release/git-ls \
  --baseline /path/to/original/git-ls \
  --directory /tmp/git-ls-diff-stat --repeats 9
```

Use `--backend shell` to repeat the experiment with the command backend.
Use `--cases` to select named fixtures or omit `--baseline` for candidate-only regression measurements.
Stop other builds and benchmarks before collecting timing evidence.

Rename similarity and textual comparison still incur Git's workload-dependent costs when counts must be calculated.
The implementation does not change rename limits or comparison semantics to reduce those costs.
Cold statistics-cache validation can consequently cost more than one uncached batch, particularly for large edited renames.
Warm context validation still launches an attribute query for non-empty path sets.
Subprocess output remains fully buffered, and exceptionally large path sets or metadata can exceed cache admission limits and require live calculation on subsequent invocations.
These bounds and residual costs preclude a universal latency or monotonic speedup guarantee.

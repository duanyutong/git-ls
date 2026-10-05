# Performance implementation and evaluation

**Date:** 5 October 2026. **Implementation:** `dyt/perf-reuse`, following baseline `3b31462` (version 0.10.8). **Scope:** Exact graph analysis, diagnostic attribution, automatic repository-local reuse, and bounded deferred cache maintenance.

## Algorithmic changes

The principal source of the reported slowdown was repeated ancestry analysis. Rendering the annotation `(X commits on main)` was inexpensive; computing its count independently for each attachment point was not.

| Operation | Implemented behaviour | Bound and qualification |
| --- | --- | --- |
| Branch selection and independent heads | The native backend walks the union of candidate histories excluding main history once. The shell backend queries captured object IDs through standard input and uses a collective independent-head query, with a shared range walk for large argument sets. | Application processing is linear in the visited union and its edges. The former pairwise head-comparison loop has been removed. Library traversal costs remain observable only where exposed. |
| Lane ancestry | Classify descendants using the edges in the collected revision range; preserve the original revision ordering when filtering. | `O(V + E)` classification and storage. No additional search below excluded boundaries. This also corrects the redundant-parent false negative. |
| Merge-base reuse | Propagate the outside-parent boundary of the complete selection range. A unique boundary proves the merge base; no boundary proves unrelated history. | One topological pass. Ambiguous boundaries retain the backend's existing multiple-base policy. Proof retention stops above 65,536 selection vertices, after which normal merge-base search remains available. |
| Main-history distances | Traverse a verified single-parent corridor once; use shared DAG propagation otherwise. | Linear corridor: `O(max(dᵢ) + G)`. General DAG: masks are processed in blocks of 64 bases, with `O(V + E + G)` storage; counting set bits can still require `O(VG)` work. |
| Shared work | Retain bounded parent records and directional query results, memoise branchless rewrite queries, batch metadata, reuse branch-point extraction, and cache group timestamp extrema. | At most 65,536 entries per invocation memo category. Complete lane vectors are not persisted. Query-specific ancestry flags are never reused for another base. |
| Initialisation | Retrieve all application configuration through one Git subprocess. | Preserves Git's include processing and value precedence. Repository discovery and reference validation remain necessary. |

The selection proof is conservative. Several outside parents may ultimately possess one best merge base; that case still uses ordinary merge-base search. Commit timestamps are never treated as ancestry proofs. Strict-descendant counts retain Git's ancestry-path semantics, including side branches of merges; they are not first-parent distances.

The shell backend retains SHA-256 object-ID support without enabling a larger native object-ID representation globally. Persistent reuse conservatively bypasses repositories which the native discovery dependency cannot interpret, including SHA-256 repositories in this build. Explicit shell rendering remains available.

## Diagnostic contract

`--debug` retains contextual progress messages and adds `git-ls debug: perf` records to standard error. Rendered standard output is unchanged. Durations use a monotonic clock and milliseconds, formatted to three decimal places; this precision does not imply equivalent measurement accuracy.

Stages cover configuration, discovery, reference capture, selection, lanes, rewrite detection, groups, formatting, output, cache validation, loading, admission, publication, and maintenance scheduling. Per-lane records identify the head, selected base, returned path size, and visible branch points. Batched distance work records its strategy and resulting base counts. Operations and counters distinguish subprocess families, merge-base searches and reuse, ancestry results, parent loads, metadata reuse, and retained entries.

Totals are inclusive. A nested operation must not be added to its enclosing stage. `observed_total_ms` begins after argument parsing and ends before summary emission; external measurement is required for process launch, terminal detection, parsing, and final diagnostic output. `stage=write_output ... status=ok` is emitted after successful flushing, before cache publication and maintenance scheduling. Error completion preserves the primary error and status.

Disabled diagnostics create no recorder, read no diagnostic clock, update no diagnostic counters, and format no diagnostic records. Small conditional checks remain. Debug mode has measurable formatting and output costs; the implementation does not claim zero overhead. Parent-load counts describe application calls, not physical disk reads or all internal gix activity. Peak-entry counters are not resident-memory measurements.

## Persistent and progressive reuse

The default ordinary selector stores disposable analysis at `<git-common-dir>/git-ls/cache/v1/`, normally `.git/git-ls/cache/v1/`. Linked worktrees share graph facts whilst current-branch and detached-HEAD markers remain live. No tracked files or global service are required.

An exact snapshot contains neutral lane topology and raw metadata. Its key covers backend and analysis compatibility, object format, canonical repository location, configured main selection, and the sorted mapping of full local reference names to full object IDs. Ordering, layout, verbosity, colours, relative ages, and worktree markers are applied afresh. Successful directional merge-base, distance, and metadata facts survive snapshot invalidation when their object-ID dependencies remain valid. Each category retains up to 512 recently used entries at publication, allowing ordinary editing sequences to retain useful work.

Inputs are frozen for analysis and checked again before publication. Main configuration is independently re-read because the native repository handle retains its initial configuration snapshot. A concurrent ref or relevant configuration change prevents publication. Failed or incomplete graph queries never become authoritative negative cache entries.

Storage comprises 32 snapshot slots, two backend fact files, and one temporary publication, each capped at 1 MiB. The admitted data bound is therefore 35 MiB, plus bounded bookkeeping, even with maintenance disabled. Publication is atomic and uses non-blocking locks. Slot collisions, corrupt or incompatible entries, oversized results, contention, and unavailable storage cause a miss or skipped publication rather than command failure.

Persistent reuse bypasses custom or hidden branchless selections, shallow history, replacement refs, grafts, alternate stores, promisor or partial-clone configurations, and unsupported graph-related environment overrides. These cases retain their existing backend behaviour and the applicable uncached optimisations. Repacking alone does not invalidate logical analysis. Persistent branchless state and parent-object indexing were assessed but excluded: the former lacks a sufficient ref-only validity key, and the latter was not justified by the residual profile.

Controls are `--no-cache`, `--refresh-cache`, `--clear-cache`, and `--no-cache-maintenance`. Configuration keys `git-ls.cache` and `git-ls.cacheMaintenance` may disable their respective facilities. Explicit refresh overrides configured cache disablement for that invocation. Bypass skips persistent reads and writes; clearing affects only this version's owned namespace and renders uncached.

## Deferred maintenance

After successful output flushing and bounded publication, maintenance may be scheduled after sixteen publications or 4 MiB of admitted snapshot data, subject to a one-hour minimum interval. Scheduling and worker locks are non-blocking and shared by worktrees. A short-lived detached helper uses null standard streams; Unix requests reduced priority through `nice`, and Windows requests detached execution and below-normal priority.

A pass examines at most 32 snapshot slots, reclaims at most 8 MiB, and observes a cooperative 100 ms time budget. Filesystem calls cannot themselves be interrupted by that budget. Retention favours eight recent snapshots and the triggering snapshot; seven-day-old entries are eligible for removal. The worker removes abandoned temporary publication files only whilst holding the publication lock. It performs no Git object GC, network work, or speculative history traversal.

The foreground does not wait for worker completion. Publication remains immediately reusable regardless of worker success. Scheduling reservations coalesce attempts and permit retry after interruption. Debug output distinguishes scheduling decisions from the previous worker's completion report. Disabled maintenance, failed spawning, worker termination, or lock contention cannot defeat the independent storage bound.

## Evaluation

Measurements used release builds on macOS 26.6.2, Apple Silicon, Rust 1.99.0, and gix 0.88.0. Each mode received a warm-up followed by nine fresh-process samples in reproducibly shuffled order, with ordinary filesystem caches warm. Standard output was discarded and diagnostic output captured separately. The baseline was built from `3b31462`; the candidate was the validated 0.13.2 working tree. Binary hashes, versions, every latency sample, child CPU time, and peak resident memory are retained in [end-to-end evidence](performance-evidence/end-to-end.json).

| Synthetic repository | Baseline | Improved, cache disabled | Forced refresh | Warm reuse |
| --- | ---: | ---: | ---: | ---: |
| 10,000 main commits; one tip at main | 64.4 ms | 23.7 ms | 23.4 ms | 23.8 ms |
| 10,000 main commits; 32 distributed bases | 717.9 ms | 48.4 ms | 51.7 ms | 32.8 ms |
| 50,000 main commits; eight distributed bases | 1,775.4 ms | 127.6 ms | 145.1 ms | 35.5 ms |
| 10,000 main commits; 128 tips at main | 183.0 ms | 41.3 ms | 46.1 ms | 38.8 ms |

Values are medians, not latency guarantees. For the deepest fixture, uncached samples ranged from 120.2 to 144.9 ms and warm samples from 24.7 to 44.4 ms; baseline samples ranged from 1,655.5 to 2,361.5 ms. Median peak resident memory was 23.75 MiB for the baseline, 20.70 MiB uncached, 20.98 MiB during refresh, and 6.72 MiB warm. Refresh added approximately 17.5 ms over the uncached median on that fixture; this includes validation, complete metadata, admission, and publication. Small or fresh-tip cases have much less reusable work, as the table demonstrates.

The [deep-history diagnostic record](performance-evidence/deep-history-debug.txt) independently establishes reduced work: eight selection vertices, eight merge-base answers proved by selection, 25,000 direct parent loads for one shared main corridor, and 25,008 peak retained parent entries. The former per-base count paths totalled 112,500 entries on the equivalent spacing. Exact snapshot hits issue no selection or lane-ancestry queries. The final profile still attributes substantial uncached time to the necessary shared corridor; warm reuse eliminates that graph analysis.

### Diagnostic overhead

A separate nine-sample comparison isolates the initial timing feature (`28cf6ed`, version 0.11.0) against the pre-change binary, before subsequent algorithmic changes. On the one-tip fixture, disabled instrumentation changed the median from 62.76 to 63.09 ms: approximately +0.33 ms, or +0.5%, within overlapping sample ranges. Existing debug output changed from 62.08 to 65.73 ms: approximately +3.65 ms, or +5.9%. Larger fixtures had overlapping ranges and mixed small differences. The [isolation evidence](performance-evidence/diagnostics-isolation.json) retains all samples; its later `final_*` fields refer to the intermediate 0.13.1 build and must not be confused with the final table above.

In the completed build, the 32-base fixture measured 48.42 ms without debug and 49.34 ms with debug when uncached, approximately +0.92 ms or +1.9%. Warm medians were 32.78 and 33.21 ms, approximately +0.43 ms or +1.3%. The deepest fixture showed larger median differences, +18.68 ms uncached and +4.85 ms warm, with overlapping ranges. These measurements do not establish a fixed logging cost or a statistically precise overhead bound.

A separate runtime experiment compared piped debug output with a continuously drained pseudo-terminal: medians were 23.09 and 23.81 ms respectively, approximately +0.72 ms or +3.1%. This measures transport and terminal-driver effects; a real terminal emulator, terminal rendering, or a slow consumer may cost more. It is not valid to describe all debug output as imperceptible.

### Maintenance interference

The runtime experiment populated a disposable namespace with approximately 31 MiB of additional snapshot data, then started the reduced-priority worker immediately before a warm foreground invocation. All nine samples overlapped worker and foreground lifetimes. Foreground debug medians were 23.09 ms without a worker and 27.01 ms with a worker: approximately +3.92 ms, or +17.0%, on this short invocation. Receipt of the post-flush record occurred at median 22.70 and 26.50 ms respectively. Record receipt is an externally observed proxy for completed rendering, not the exact flush system-call timestamp.

Each pass examined 32 slots and reclaimed exactly 8 MiB. The recorded maintenance pass lasted 0.70–1.29 ms; complete helper process lifetime was 14.26–21.97 ms, including launch. Helper CPU time was 6.00–8.38 ms and peak resident memory approximately 1.84–1.88 MiB. The worker generated no terminal output. [Runtime evidence](performance-evidence/runtime.json) retains foreground, worker, and reclamation measurements separately.

The result supports infrequent, bounded maintenance with an explicit disable control. It does not justify a claim of zero contention. A separate integration test deliberately blocks a worker until after foreground exit and pipeline EOF, establishing non-waiting behaviour independently of the short measured worker lifetime.

### Reproduction

The harnesses create disposable synthetic repositories and isolate Git configuration. The primary harness records every sample, version, binary hash, CPU time, and peak resident memory. The runtime harness requires Unix pseudo-terminals and `wait4`; its synthetic cache mutations remain below the supplied scratch directory.

```sh
cargo build --release --locked --package git-ls
python3 scripts/benchmark-performance.py \
  --candidate target/release/git-ls \
  --baseline /path/to/baseline/git-ls \
  --directory /tmp/git-ls-performance --repeats 9
python3 scripts/benchmark-runtime.py \
  --candidate target/release/git-ls \
  --directory /tmp/git-ls-runtime --repeats 9
```

Omit `--baseline` for ongoing regression measurements. Build a comparison binary from an isolated checkout of the intended baseline with the same compiler and dependency lockfile. Stop other builds and benchmark processes before collecting timing evidence. None of these figures measures the repository from the original user transcript.

## Correctness and validation

The graph suite exhaustively enumerates every topologically labelled five-vertex DAG, comparing ancestry classification and counts against independent ancestor searches and checking boundary proofs against explicit outside-parent sets. Additional fixtures cover mask blocks beyond 64 bases, redundant parents in either order, merged side histories, unrelated roots, shallow roots, nested and incomparable bases, and multiple best merge bases. Actual Git supplies the backend parity oracle.

Integration coverage compares cached and uncached output across both backends, layouts, ordering and verbosity; exercises branch changes, main advancement, detached HEAD, linked worktrees, graph-interpretation changes, corruption, unavailable storage, and concurrent publication; and checks incremental fact reuse. A deliberately blocked maintenance helper establishes that the foreground and output pipeline terminate independently. Maintenance tests cover coalescing, active publications, retention, byte quotas, and repeated passes.

The repository gate is `just check`. Package coverage retains its 95% minimum; the strict unit gate requires zero uncovered lines, regions, and functions in pure logic. New filesystem and process adapters join the existing adapter exclusions from the strict unit boundary and remain subject to package coverage and integration tests. The coverage thresholds themselves have not been reduced. Final package line coverage was 98.24% for `git-ls` and 98.28% for `xtask`; the strict unit scope achieved 100% lines, regions, and functions. Both `just check` and `just coverage` passed on the completed implementation. Platform-specific Windows process flags were not executed on the macOS validation host.

## Residual costs and limits

Reference enumeration, repository discovery, snapshot validation, and fresh rendering remain on warm invocations. Small repositories can incur cache overhead without a compensating traversal saving; `--no-cache` is available for one-shot use and controlled comparison. The reusable working set is bounded, so eviction or new history can make a later invocation slower. No monotonic speedup guarantee is made.

Per-lane ordering walks and ambiguous merge-base searches remain exact backend operations. Large general DAGs can still require substantial work and graph storage. Existing library commit-graph acceleration and the 4 MiB object cache are retained; there is no automatic Git maintenance, backend switching, or graph-analysis parallelism. The measured reductions in repeated work do not justify those additional mechanisms at present.

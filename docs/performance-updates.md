# Performance updates for local evaluation

This report describes the local candidate following version 0.16.0.
The investigation and retained baseline measurements are recorded in `.agents/plans/git-ls-performance-20261027.md`.
The changes preserve endpoint-based statistics, Git's effective attribute semantics, directional comparisons and the independent cache storage bound.

## Statistics acquisition and reuse

Statistics are retained by ordered endpoint tree pair within a bounded worktree and executable context.
Immutable commit-to-tree mappings preserve the cheap unchanged-commit path whilst allowing metadata-only amendments to reuse existing comparisons.
At most four contexts and 4,096 comparison entries are retained across the facts entry, subject to its existing 1 MiB encoded limit.
Graph facts remain shared between linked worktrees, and worktree-specific attribute results remain independent.
Eviction favours recently used entries without forcing a write on every unchanged invocation.

Each comparison retains its own dependency paths and a fingerprint of their effective attributes and relevant configuration.
Validation queries only paths required by the current selection; attributes affecting exclusively unrequested comparisons do not invalidate requested answers.
Git continues to perform attribute resolution, including worktree, index fallback, repository-info, global and system precedence.
The validity fingerprint conservatively retains unknown Git configuration whilst excluding application presentation settings, `core.editor` and `core.pager`.

On a statistics miss, the native backend discovers changed paths through an attribute-independent object-tree walk.
The shell backend obtains the same dependency superset through a framed raw tree comparison with rename detection disabled for discovery alone.
Both old and new rename paths participate in validation, and the numeric comparison retains the established rename policy.
Raw record framing distinguishes paths containing tabs, newlines or strings resembling comparison headers.
Counts are calculated once between context captures, rather than using a first numeric comparison to discover dependencies.
If dependency names alone exceed the cache-entry byte limit, admission is bypassed before context capture and a single live numeric batch produces the result.
Context changes prevent admission; partially reused answers are recomputed collectively if their validity cannot be maintained.

## Topology and history

Topology snapshots intern common branch-point paths and store bounded references to their shared suffixes.
References are checked during decoding, and invalid or cyclic paths invalidate the optional snapshot.
This reduces repeated storage for named shared trunks while preserving distinct paths through merged history.

The native backend summarises complete single-parent selection ranges collectively.
Shared corridors are processed once, and each head materialises its branch boundaries and exact interval counts from those summaries.
Merged ranges, incomplete retained ranges and unsupported cases continue through the existing ancestry traversal.
The summary does not use timestamps as ancestry evidence or replace ancestry-path counts with first-parent counts on merged graphs.

## Live branchless queries

Custom revsets and hidden selections retain live branchless event-state interpretation.
For ordinary-sized branch selections, a collective `current` query over the exact ancestry range establishes whether any interior commit resolves to a selected branch point.
Unchanged interiors consequently require one query for the range rather than one query per commit.
When relevant rewrites exist, bounded adaptive queries resolve their individual associations without inventing mappings for ambiguous or empty results.
Oversized expressions use bounded groups, and repeated results remain memoised only within the invocation.
An isolated hook-enabled repository verifies baseline-equivalent rendering after an actual interior amend with both backends and both ordinary and hidden selections.

## Presentation and publication

Terminal width preflight and final output reuse one prepared topology and colour assignment.
The renderer styles only lane cells which can contribute to the visible prefix, retaining spacing required for the established layout and width rules.
Hidden lane cells avoid redundant colour instructions, and truncation removes overwritten style changes which contain no visible text.
Visible padding retains its established style so that truncation markers preserve their original colour.
Unicode width, visible glyph colours, fixed suffixes, truncation markers and terminal reset behaviour remain part of the rendering contract.
Output is buffered until the existing explicit flush boundary.
Diagnostics report prepared topologies, rendered lane cells and emitted output bytes alongside statistics dependencies and tree-cache hits.

Facts admission retains the encoded bytes used to establish the byte bound and reuses them for publication.
The unchanged-answer comparison uses the originally loaded facts instead of reopening and reparsing the cache entry.
The analysis compatibility identifier is incremented so that prior snapshots and fact representations are not interpreted under the new contract.

## Evaluation

The maintained `scripts/benchmark-diff-stat.py` harness includes metadata-only amendments, same-path and new-path changes, unrelated settings, worktree alternation, large dependency sets, shared named and unnamed trunks, and custom branchless selections.
Mutating cases apply the same deterministic event to each isolated repository before timing and compare baseline and candidate output for that event.
Terminal probes disable pseudo-terminal output translation to permit stable byte comparisons.
Absolute timings require a quiet host and shall be reported separately from deterministic operation counts and output-size reductions.

The local comparison covers 27 fixtures with three fresh-process samples per mode, reproducibly shuffled invocation order and separate cache namespaces for the baseline and candidate.
Preparatory invocations establish the intended cache state outside the timed interval for attribute changes, topic creation and worktree switching.
The native comparison verifies equivalent visible output and non-space glyph colours, including narrow and coloured terminal probes.
Host load varied during measurement; these samples establish exploratory medians rather than a statistically precise latency guarantee.
The focused shell comparison supplements the functional acceptance suite with nine operation-count and output-parity fixtures and one sample per mode.
Its single samples do not establish shell-backend latency estimates.

| Workload and measured mode | Baseline median | Candidate median |
| --- | ---: | ---: |
| One unchanged layer, warm statistics | 40.18 ms | 54.05 ms |
| 32 unchanged layers, warm statistics | 54.32 ms | 51.58 ms |
| 32 independent layers, cold statistics | 151.06 ms | 123.64 ms |
| Same-path tip edit amongst 32 layers, warm cache | 193.95 ms | 135.48 ms |
| New-path tip edit amongst 32 layers, warm cache | 208.49 ms | 114.04 ms |
| New topic beside 32 layers, warm cache | 181.42 ms | 111.70 ms |
| Metadata-only amendment, warm cache | 223.38 ms | 54.06 ms |
| Unrelated editor setting change, warm cache | 192.62 ms | 47.26 ms |
| Alternating warmed worktrees, statistics enabled | 245.48 ms | 59.31 ms |
| Attribute change affecting only an unselected path, warm cache | 158.01 ms | 47.95 ms |
| 320 named shared points and 32 heads, warm statistics | 109.93 ms | 82.94 ms |
| 5,000 shared commits and 32 heads, cache and statistics disabled | 191.06 ms | 52.06 ms |
| 6,000 long dependency paths, warm topology and live statistics | 1,632.42 ms | 858.64 ms |
| Custom branchless selection with 31 interior candidates, statistics disabled | 2,340.56 ms | 265.53 ms |

Unchanged small selections do not demonstrate a uniform speedup.
The single-layer warm result is slower in this sample set, whilst the 32-layer warm results are close.
The operation-count reductions provide stronger attribution for the development-event and capacity cases than small differences between noisy elapsed medians.
The following operation counts refer to the native backend.

| Deterministic probe | Baseline | Candidate |
| --- | ---: | ---: |
| Same-path edit: numeric batches / Git processes | 2 / 10 | 1 / 5 |
| Metadata-only amendment: numeric batches / Git processes | 2 / 10 | 0 / 2 |
| Warm worktree switch: numeric batches / Git processes | 2 / 8 | 0 / 2 |
| Oversized dependency set: numeric batches / Git processes | 2 / 6 | 1 / 2 |
| Shared 5,000-commit corridor: repeated lane visits / collectively summarised vertices | 160,032 visits | 5,032 vertices |
| Unchanged branchless interior: rewrite queries | 31 | 1 |
| 512 lanes at 120 columns: coloured output bytes | 3,879,565 | 499,634 |
| 512 lanes at 120 columns: ANSI sequences | 527,373 | 53,361 |

The coloured-output reduction is approximately 87.1%, with matching visible text and glyph colours.
These byte counts concern a continuously drained pseudo-terminal and do not measure terminal-emulator rendering latency.
The 320-point shared snapshot is now admitted and reused within the existing entry limit.
Shared ancestry preparation occurs once per selection, including the 512-independent-head fixture; it does not rebuild the branch-point set for every head.

The retained local evidence resides in `.agents/plans/git-ls-performance-20261027/implementation/`, including executable identities, raw samples, Git traces, diagnostic records and validation logs.
The initial candidate comparison is retained separately; it exposed repeated branch-point preparation, which was removed before the final build.
The inactive-path fixture was rerun after correcting its preparation so that the timed invocation, rather than a preparatory invocation, observes the attribute change.
`just check` and `just coverage` pass for the final implementation, including complete line, region and function coverage of the 39 designated unit modules.

The initial investigation's projections are not guarantees of the candidate's eventual measured latency.
Git process startup, attribute resolution, expensive content or rename comparisons, cache-capacity limits and terminal-consumer behaviour remain workload-dependent costs.
Replacing Git's effective attribute query and redesigning general-DAG distance analysis remain conditional on semantic parity and residual profiling evidence, as specified in the plan.

## Shallow repository reuse

A subsequent local update addresses the conservative shallow-history bypass in version 0.16.1.
A shallow boundary changes ancestry semantics independently of branch tips: fetching additional history may reveal parents of a previously truncated commit without changing its object ID.
The earlier compatibility key did not represent that mutable graph boundary, so both graph and statistics persistence were disabled for shallow repositories.

The compatibility key now includes the sorted, deduplicated shallow-boundary object IDs for both facts and topology snapshots.
An unchanged boundary permits normal cache admission and warm reuse, while deepening, unshallowing or changing boundary contents establishes a distinct compatibility context.
The native backend's captured boundary must agree with the live cache context before any persisted result is read.
Publication revalidates the live context after analysis and discards results if the boundary changed during execution.
Boundary identity derives from parsed content rather than modification times; reordering or duplicating equivalent boundary records does not invalidate results.
Linked worktrees share the common repository's boundary while retaining their existing worktree-specific statistics validation.
Malformed boundary data and unsupported graph overrides continue to bypass reuse or fail through the existing Git error handling.

Diagnostics now identify `cache_graph=shallow shallow_boundaries=N` when a shallow repository is eligible.
An unchanged warmed invocation should additionally report `cache=hit` and statistics hits without selection traversal or a numeric diff subprocess.
Tests cover real shallow fetches followed by deepening and unshallowing with unchanged local refs, linked worktrees, malformed metadata, stale native handles and boundary changes during execution.
The maintained benchmark adds shallow clones at depths 1, 2,500 and 5,000, using isolated object stores and baseline-equivalent output checks.
The long-main fixture includes four merged branch tips and reproduces the reported selection dimensions: 22 local tips, 17 selected commits and eight independent heads.

The following medians compare the committed version 0.16.1 with the local candidate.
Native cases use nine fresh-process samples per mode; the shell case uses three.
Invocation order is reproducibly shuffled within each fixture.
Baseline shallow invocations bypass persistence, while candidate warm invocations reuse previously admitted entries.

| Fixture | Backend | Baseline repeated statistics | Candidate warm statistics | Candidate cold statistics |
| --- | --- | ---: | ---: | ---: |
| 32 independent shallow branches, depth 1 | Native | 131.65 ms | 73.62 ms | 196.93 ms |
| Shared 5,000-commit source history, depth 2,500 | Native | 135.56 ms | 102.42 ms | 201.01 ms |
| Long main history with 22 tips and 17 selected commits, depth 5,000 | Native | 77.37 ms | 69.92 ms | 153.44 ms |
| 32 independent shallow branches, depth 1 | Shell | 1,668.35 ms | 300.61 ms | 1,526.17 ms |
| 32 independent branches with complete history, control | Native | 79.72 ms | 80.70 ms | 226.18 ms |

The first eligible invocation may cost more because it captures validity context and publishes reusable results.
The benefit applies to subsequent invocations with unchanged inputs.
Host load varied substantially between batches, including observed load averages between approximately 14 and 35, so these elapsed values remain exploratory.
The complete-history control shows similar warm latency, and the fixture matching the reported selection counts does not reproduce the remote repository's content-comparison or object-loading costs.
These measurements therefore do not establish a latency forecast for `av`.

Deterministic probes establish the intended change independently of timing noise.
All three native shallow fixtures report warm topology hits, reuse every requested statistic and execute no selection traversal or numeric diff.
The 22-tip fixture reuses all 17 statistics.
The shell fixture reduces Git processes from 75 to six and executes no numeric diff after warming.
Both backends retain baseline-equivalent visible text and glyph colours.
`just check` passes all 363 Rust acceptance tests, and `just coverage` retains 100% line, region and function coverage across the 39 designated unit modules.
The additional benchmark fixtures pass formatting, lint, type and output-parity checks.
Retained evidence resides in `.agents/plans/shallow-cache-20261027/`.

## Local trial

Build with `just build-release` from this project's root, then invoke the resulting `target/release/git-ls` by its absolute path from the repository being evaluated, with the usual command-line options.
The installed executable is independent of this local build.
The changed analysis compatibility identifier requires one initial cache reconstruction; subsequent unchanged invocations exercise warm reuse.

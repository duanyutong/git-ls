# Statistics performance programme

The statistics performance programme preserves directional endpoint-tree counts, effective Git configuration and attributes, rename semantics, and the existing cache storage limits.
Its implementation is divided into measurement, diagnostics, admission, configuration validity, dependency storage, bounded fallback, reference lifecycles, selective recomputation, cold acquisition, warm validation, and integrated regression evidence.

## Measurement contract

The maintained `scripts/benchmark-diff-stat.py` harness records whole-process elapsed time, child CPU consumption and peak resident memory where available.
Every sample includes host conditions before and after execution, including available processor count and load averages where the operating system exposes them.
Unavailable load telemetry is represented explicitly.
The ratio of load average to available processors provides context; it does not establish the cause of waiting or prove that a host was uncontended.
Elapsed time minus child CPU consumption shall not be interpreted as a measured breakdown of scheduling and I/O delays.

Results retain each raw sample, the reproducibly shuffled measurement order, executable identities, output digests, Git traces and diagnostic counters.
Summaries include the median, range, nearest-rank 95th percentile, and median CPU consumption across samples for which CPU data are available.
Percentiles based on fewer than thirty samples are exploratory.
Use separate baseline and candidate cache namespaces, equivalent fixtures and serial execution without competing builds or benchmarks.
Compare deterministic operation counts before attributing an elapsed-time difference to an implementation improvement.
An uncontended comparison is required for a precise latency claim.

The `--cases` and `--modes` options permit focused comparisons without changing the default fixture set.
For example:

```sh
uv run scripts/benchmark-diff-stat.py \
  --candidate /path/to/candidate/git-ls \
  --baseline /path/to/baseline/git-ls \
  --directory /tmp/git-ls-statistics \
  --cases independent-1 stack-32 \
  --modes warm_on cold_on \
  --repeats 9
```

Functional acceptance shall continue to use disposable repositories, isolated Git configuration and output equivalence checks.
Machine-dependent latency thresholds do not form part of correctness CI.

## Cache diagnostics

Debug output identifies each directional tree pair and its context, distinguishing absent contexts, absent comparisons, changed effective configuration, changed effective attributes and valid reuse.
Admission records retained and rejected comparisons, encoded facts size and eviction causes.
Encoded publication reports successful writes, unavailable locks or storage, and write failures; reference revalidation retains its explicit discarded-publication event.
The records contain identifiers and reasons without configuration values or file contents.
Aggregate durations remain inclusive; individual numeric comparisons require independent measurement when Git processes them in one batch.

## Facts admission

Statistics requested by the current invocation take precedence over historical comparisons when applying entry-count and byte limits.
Byte admission selects a deterministic retained prefix with logarithmically many complete-envelope checks and moves discarded values without repeatedly cloning dependency paths.
Historical growth therefore does not discard the current set when that set fits alongside higher-priority graph and metadata facts.
An oversized current set retains a bounded useful subset; commit-to-tree mappings are trimmed separately when necessary.
Invocation-local protection is neither serialised nor treated as a change in cached answers.

## Configuration validity

Numeric statistics ignore audited configuration namespaces governing branch metadata, transport, identity and credential selection, together with the existing application and editor or pager exclusions.
Git continues to resolve includes, scope precedence and effective values before classification.
Unknown configuration remains part of the fingerprint, and meaningful ordering of retained values is preserved.
Topology continues to use its independent captured branch and upstream inputs.

| Input class | Statistics validity | Basis |
| --- | --- | --- |
| git-ls.*, core.editor, core.pager | Retained. | Application presentation and interactive editor or pager selection do not determine numeric counts. |
| branch.*, fetch.*, push.*, pull.*, remote.* | Retained. | Branch metadata and transport policy do not change the immutable directional tree comparison. |
| user.*, credential.* | Retained. | Identity and credential selection do not determine local numeric diff results. |
| diff.*, core.attributesFile, other core settings | Revalidated. | These can determine comparison policy, binary classification or object and attribute interpretation. |
| Unclassified settings | Revalidated. | Unknown dependencies remain conservative. |

Changes in effective attributes retain their independent path-specific invalidation.
The comparison continues to pass --no-ext-diff, --no-textconv, --find-renames, --no-relative and --no-color explicitly.

## Dependency storage

Persistent statistics intern distinct path strings and identical ordered dependency sets across retained comparisons and contexts.
Decoded comparisons referencing an identical dependency set share its immutable allocation, reducing the cost of copying retained facts and selecting dependencies.
Publication constructs pools only from retained comparisons, so evicted entries leave no unreachable persistent paths or dependency sets.
All pool identifiers are validated during decoding, and an incompatible representation invalidates prior optional cache entries.
The existing 1 MiB encoded-envelope limit remains effective.
Both encoding and decoding additionally bound unique expanded dependency storage to 16 MiB, including string-slot overhead, so repeated references cannot cause unbounded allocation.
The existing four-context and 4,096-comparison bounds are also enforced at the representation boundary.

## Oversized selections and fallback

An oversized dependency union does not require every comparison to bypass the statistics cache.
The application first selects a bounded validation set, preferring retained comparisons and then comparisons with larger dependency sets, with deterministic tie-breaking.
Only comparisons covered by the validated effective context may supply cached answers or acquire new persistent counts.
Remaining comparisons share the ordinary numeric batch, preserving accurate complete output without expanding the attribute-validation input beyond its bound.
The before-and-after context check remains mandatory for admitting newly measured statistics, and all discovered dependency paths remain available for detecting incomplete coverage.
An unstable context with partial hits still requires a complete live recalculation to avoid combining incompatible answers.
When no valid context can be established, the application performs one complete live batch without admitting unvalidated counts.
An unchanged retained subset does not force repeated facts publication merely because another comparison remains too large to retain.
Diagnostics distinguish encoded-envelope exhaustion from a representation-bound failure.

Within active and historical admission tiers, smaller encoded dependency payloads take precedence, with recency retained as a deterministic tie-breaker.
The estimate accounts for both JSON escaping layers and is computed once per shared dependency allocation for each ranking pass.
This prevents an individually oversized leading comparison from blocking smaller comparisons which collectively fit.
Exact envelope checks remain authoritative and logarithmically bounded.

Encoding recognises shared dependency allocations before enumerating paths or constructing path identifiers, avoiding repeated hashing of identical decoded sets across historical comparisons.
Equal dependency contents still intern identically when their runtime allocations differ.
Decoding reserves the known dependency and comparison collection sizes within the existing representation bounds.

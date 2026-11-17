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

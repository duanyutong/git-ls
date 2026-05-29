# Modularisation and Coverage Plan

## Purpose

This document records structural, design, organisational, implementation, style,
and readability observations identified during the initial modularisation of
`git-ls`. It is intended to support subsequent coverage work by multiple agents
without requiring each agent to rediscover the same architectural constraints.

## Initial Observations

- `src/lib.rs` presently contains the command-line interface, configuration
  parsing, Git command abstraction, shell-backed Git operations, `gix`-backed
  Git operations, repository lane construction, rendering, terminal-width
  handling, public execution orchestration, and unit tests in a single file.
- The single-file arrangement makes ownership boundaries implicit. A reader must
  infer which functions form the Git access layer, which functions form pure
  layout logic, and which functions form terminal integration.
- The test suite is comprehensive in several behavioural areas, but its
  placement beside every private item encourages broad private coupling. That
  arrangement obstructs module-level coverage ownership.
- The executable public surface is intentionally small: `run_from_env()` and the
  public error/result types are the only obvious stable crate boundary. The
  refactor should preserve that surface unless an explicit library API is later
  designed.

## Target Module Boundaries

- `error`: crate error and result types.
- `model`: pure data structures and object-id display helpers.
- `cli`: command-line argument types, effective argument resolution, palette
  selection, and Git configuration parsing.
- `backend`: Git process abstraction, shell-backed operations, `gix`-backed
  operations, and commit metadata retrieval.
- `lanes`: branch-point discovery, lane construction, ordering, and grouping.
- `render`: colour application, textual graph layout, metadata-width
  calculation, and row rendering.
- `terminal`: terminal width discovery and output-line truncation.
- `app`: top-level parsing, configuration loading, execution orchestration, and
  public `run_from_env()` delegation.

## Issue Register

This register is retained as an audit trail. Some entries describe issues that
have subsequently been addressed by the completed PR sequence recorded below;
the active work queue is the authoritative list of remaining planned work.

### API

1. **A1: Error types mix public semantics and backend diagnostics.**
   Error variants are materially tied to implementation details of both Git
   backends and CLI parsing. Future public API work should distinguish stable
   user-facing error semantics from backend-specific diagnostics.
2. **A2: CLI parsing and effective-option construction may need
   separation.**
   The CLI module now owns palette names, defaults, argument parsing, and
   config resolution. That is a coherent first boundary, but future library API
   work may require separating raw CLI parsing from effective runtime option
   construction.
3. **A3: The public execution surface should remain deliberate.**
   `app::run_from_env()` remains the only public execution entry point. That is
   appropriate for the present CLI crate, but any future library use should be
   designed deliberately rather than by exposing the current internals
   piecemeal.

### Architecture

1. **B1: Lane construction couples repository queries and lane
   derivation.**
   The lane-building layer couples branchless revset queries, local branch
   mapping, ancestry traversal, metadata caching, and current-branch detection.
   Focused tests would be easier if these steps were represented by smaller
   pure functions with explicit inputs and outputs.
2. **B2: Lane ordering logic is mixed with repository choreography.**
   The lane-building module owns both repository-query choreography and pure
   ordering/grouping logic. The latter should eventually be isolated from the
   former so that branch ordering and group ordering can be covered without Git
   fakes.
3. **B3: `execute()` combines orchestration, rendering preparation, and
   output I/O.**
   `execute()` still performs orchestration, metadata lookup, layout
   preparation, and output writing in one function. It should eventually be
   split into a pure render-plan constructor and an I/O writer.
4. **B4: Empty-selection rendering duplicates context construction.**
   The empty-selection path in `execute()` duplicates some context construction
   used by the populated path. A small render-session builder would reduce this
   duplication and make tests more direct.

### Backend

1. **C1: The Git backend trait is too broad.**
   It conflates branchless query execution, ordinary Git plumbing, metadata
   hydration, repository state, and ancestry operations. Subtraits or narrower
   adapter methods may become useful once coverage agents specialise by module.
2. **C2: Git process execution and output normalisation are coupled.**
   The process-backed Git command abstraction currently returns trimmed `stdout`
   strings. This is convenient, but it means newline handling, command failure
   handling, and output parsing are coupled. Coverage should distinguish command
   execution policy from output normalisation.
3. **C3: The `gix` backend is a hybrid implementation.**
   The `gix` backend still shells out for branchless-specific revset queries.
   This is acceptable behaviourally, but the hybrid backend should be documented
   explicitly in code or tests so that future contributors do not assume it is a
   pure `gix` implementation.
4. **C4: Metadata-cache insertion semantics are duplicated.**
   The shell and `gix` backends share metadata-cache semantics, but the common
   cache insertion rule is duplicated. A small cache helper would reduce
   behavioural drift and give coverage agents a single location for alias/full
   object-id cache invariants.
5. **C5: [backend] The backend remains a physically large
   multi-responsibility module.**
   Although the backend responsibilities are more explicit than in the original
   single-file implementation, `backend.rs` still contains process execution,
   shell-backed Git behaviour, `gix` behaviour, metadata parsing, metadata
   caching, and backend traits. A further split would reduce review scope and
   give coverage agents clearer ownership.

### Configuration

1. **D1: Configuration keys are not centralised.**
   Configuration keys are string constants inside parsing logic. A dedicated
   configuration-key table or typed configuration module would reduce
   duplication and improve test discoverability.
2. **D2: Runtime defaults are distributed across CLI code paths.**
   The default backend, default palette, default revset, and default verbosity
   are distributed across CLI declarations and resolution logic. Centralising
   defaults would make behavioural coverage less fragile.

### Docs

1. **E1: Code-level invariants need inline documentation; dedicated
   Markdown docs are deferred.**
   The project does not yet expose module-level documentation explaining the
   invariants each module owns. Do not create dedicated Markdown documentation
   for this issue at present; prefer concise Rust module docs, item doc
   comments, and explanatory code comments where they materially clarify local
   invariants.

### Domain Model

1. **F1: Repository facts and render-ready view models are conflated.**
   The `BranchPoint`, `Lane`, and `LaneGroup` structures are currently
   optimised for rendering consumption. If future features need additional
   analyses, a clearer distinction between repository facts and render-ready
   view models may be warranted.
2. **F2: [domain model] Model fields remain broadly crate-visible.**
   The core model types still expose most fields as `pub(crate)`. This is
   pragmatic for the present internal pipeline, but important construction
   invariants would be more discoverable if they were represented by constructors
   or focused builders rather than by unconstrained field mutation.

### Encapsulation

1. **G1: Tests still require broad crate-internal access.**
   Existing tests require extensive access to crate internals. The refactor
   makes many items `pub(crate)` as a transitional measure; those visibility
   choices should be tightened after focused module-level tests are introduced.
2. **G2: Some crate visibility exists only for tests.**
   Some `pub(crate)` visibility now exists solely for cross-module tests rather
   than for production collaboration. This is an acceptable transitional state,
   but it should not become the long-term crate design by accident.

### Organisation

1. **H1: Tests remain centralised after the production module split.**
   The refactor separates production code into modules, but the existing unit
   tests were intentionally preserved as a single behavioural suite to minimise
   risk in the first structural change. A subsequent coverage-focused change
   should divide `src/tests.rs` by module or move tests into module-local
   submodules once ownership of the new boundaries is stable.
2. **H2: The rendering module is still the largest production module.**
   It is a suitable next candidate for subdivision into colour/theme handling,
   metadata formatting, trunk/main rendering, branch-row rendering, orphan
   rendering, and terminal-independent layout orchestration.
3. **H3: Coverage ownership should follow module boundaries.**
   Coverage ownership should be assigned by the new module boundaries rather
   than by legacy test locations: `cli`, `backend`, `lanes`, `render`,
   `terminal`, `app`, `model`, and `error` each now has a concrete file-level
   target.
4. **H4: [organisation] Application tests still dominate `app.rs`.**
   The application module's production code has a coherent orchestration
   boundary, but its inline test module keeps the file large. Moving those tests
   behind a child test module would improve navigability without changing
   runtime behaviour.
5. **H5: [organisation] The plan mixes completed, deferred, and proposed
   work.**
   This plan has become both a historical record and a forward-looking work
   queue. A short reconciliation pass should distinguish completed PRs,
   remaining issues, and deferred items so future contributors do not treat
   already-completed work as pending.

### Rendering

1. **I1: Rendering responsibilities remain excessively broad.**
   The rendering layer still combines display policy, tree topology layout,
   metadata alignment, colour styling, and current-branch indication. These
   responsibilities should be further decomposed before attempting exhaustive
   branch coverage.
2. **I2: Palette ownership straddles CLI selection and rendering.**
   The colour palette data is part of CLI selection but also part of rendering
   behaviour. Its current location should be revisited if palette tests are
   expanded independently from argument parsing tests.

### Terminal

1. **J1: Terminal capability detection is split across modules.**
   The terminal module owns truncation and terminal-width discovery, but colour
   selection still performs its own terminal detection inside the rendering
   module. A single terminal-capability abstraction would make the boundary
   more regular.

### Testability

1. **K1: Wall-clock time is a direct dependency.**
   `current_unix_timestamp()` is a direct wall-clock dependency. Rendering and
   orchestration tests would be cleaner if time were injected through an
   explicit clock boundary.
2. **K2: Automatic colour selection reads process terminal state.**
   `Colours::new()` reads terminal state through `stdout().is_terminal()`.
   This implicit environmental dependency complicates deterministic tests for
   automatic colour selection.
3. **K3: Terminal width is read through global process state.**
   Terminal width is read globally in the public execution path. A future
   injectable terminal context would make truncation and output policy fully
   testable without relying on process state.

### Testing

1. **L1: Error behaviour lacks focused unit coverage.**
   The error module is now isolated, but there are no tests that assert error
   display text, source chaining, or conversion behaviour independently of
   larger workflows.
2. **L2: Configuration-loading order is only indirectly tested.**
   The configuration parser performs three independent Git config lookups in a
   fixed order. Tests currently assert that order indirectly in the empty-run
   workflow; module-level tests should assert configuration loading behaviour
   directly.
3. **L3: Rendering tests rely heavily on full-row golden assertions.**
   Several rendering tests assert complete row strings. These golden-style
   assertions are useful, but they are also brittle when layout is
   intentionally evolved. Lower-level tests for metadata, prefixes, trunk
   labels, and orphan rows should supplement them.
4. **L4: `MockGit` is coupled to exact shell command vectors.**
   The current test helper `MockGit` records raw command vectors. That is
   valuable for shell-boundary assertions, but it also couples tests to exact
   command construction. Higher-level backend fakes would make
   lane-construction tests less sensitive to shell argument ordering where
   ordering is not the behaviour under test.
5. **L5: Test data construction is field-heavy.**
   Test helpers construct `CommitMeta`, `BranchPoint`, `Lane`, and `LaneGroup`
   values manually. Builders or fixtures scoped by module would make future
   coverage additions shorter and less dependent on field-level representation.
6. **L6: Metadata hydration lacks negative-path parity coverage.**
   Shell commit metadata parsing and `gix` commit metadata hydration should
   have parallel negative-path tests. The current positive-path coverage does
   not fully exercise malformed records, invalid timestamps, missing objects,
   or backend error conversion.
7. **L7: Git repository fixtures are comparatively expensive.**
   The repository integration fixtures create temporary Git repositories
   through process commands. They validate important parity, but they are
   comparatively expensive and should remain distinct from pure unit tests to
   keep coverage agents' feedback loops short.
8. **L8: Main-branch display placeholders need helper-level coverage.**
   Main-branch rendering uses sentinel placeholders such as `-` and `--`
   through formatting helpers. These display conventions should be specified in
   tests at the helper level, not only in full-row output tests.
9. **L9: Lane-group comparator tie-breakers need isolated tests.**
   Ordering rules for lane groups encode several precedence levels in chained
   comparators. Dedicated tests for each tie-breaker would make future changes
   safer.

### Tooling

1. **M1: [tooling] `xtask` remains monolithic and weakly covered.**
   The `xtask` binary owns command dispatch, version-policy interpretation, Git
   range traversal, tag operations, and TOML mutation in a single file. Its line
   coverage is materially lower than the main crate, and its logic should be
   split before attempting strict coverage requirements.

### Coverage

1. **N1: [coverage] The current coverage gate is total-only.**
   The `just coverage` recipe establishes a useful repository-wide baseline,
   but total line coverage can mask weakly covered modules. Module-level
   thresholds or equivalent targeted reports should be introduced only after the
   remaining structural splits make ownership stable.

## Completed Development Record

The following entries are retained as implementation history. They are not an
active work queue.

1. **PR1: `refactor(tests): establish module-scoped test ownership`.**
   Covered `H1`, `H3`, `G1`, and `G2`.
2. **PR2: `refactor(cli): clarify configuration and runtime options`.**
   Covered `D1`, `D2`, `A2`, and `L2`.
3. **PR3: `refactor(error): clarify error semantics`.**
   Covered `A1` and `L1`.
4. **PR4: `refactor(backend): split git execution and metadata caching`.**
   Covered `C1`, `C2`, `C3`, `C4`, and `L6`.
5. **PR5: `refactor(lanes): isolate lane construction and ordering`.**
   Covered `B1`, `B2`, `F1`, `L4`, `L5`, and `L9`.
6. **PR6: `refactor(render): split rendering into focused submodules`.**
   Covered `H2`, `I1`, `I2`, `L3`, and `L8`.
7. **PR7: `refactor(terminal): inject render environment capabilities`.**
   Covered `J1`, `K1`, `K2`, and `K3`.
8. **PR8: `refactor(app): split orchestration from render planning`.**
   Covered `B3`, `B4`, and `A3`.
9. **PR9: `test(fixtures): rationalise repository-backed integration
   tests`.**
   Covered `L7`.
10. **PR10: `refactor(modules): reduce remaining large module surfaces`.**
    Covered the post-PR audit module-size issue.
11. **PR11: `refactor(api): tighten crate-internal encapsulation`.**
    Covered the post-PR audit encapsulation issue and residual `G1` and `G2`
    work.
12. **PR12: `test(cli): expand unit coverage for command workflows`.**
    Covered the post-PR audit command-workflow coverage issue.
13. **PR13: `refactor(api): design explicit library boundary`.**
    Covered the post-PR audit public-boundary issue and residual `A3` work.
14. **PR14: `test(coverage): add coverage reporting and baseline gate`.**
    Covered the post-PR audit coverage-tooling issue.
15. **PR15: `refactor(backend): split backend implementation modules`.**
    Covered `C5` and residual backend decomposition concerns.
16. **PR16: `refactor(app): move application tests behind a child module`.**
    Covered `H4`.
17. **PR17: `refactor(model): encode construction invariants`.**
    Covered `F2` and supported `F1`, `L5`, `G1`, and `G2`.
18. **PR18: `refactor(xtask): separate version policy from repository
    plumbing`.**
    Covered `M1`.
19. **PR19: `chore(plan): reconcile completed and remaining refactor work`.**
    Covered `H5`.
20. **PR20: `test(coverage): add module-level coverage targets`.**
    Covered `N1`.

## Deferred Decisions

1. **Dedicated Markdown documentation remains deferred.**
   Standalone Markdown documentation derived from `E1` remains out of scope.
   Active documentation work should continue to prefer concise Rust module
   documentation, item documentation, and explanatory code comments where they
   clarify implementation invariants.
2. **Compiled-binary end-to-end testing remains deferred.**
   Process-level tests that execute the compiled `git-ls` binary with a fake
   `git` executable on `PATH` remain out of scope. Unit tests with explicit
   mock backends and mock command data remain preferred. Temporary repositories
   should be retained only where real repository semantics are essential. The
   final coverage gate should therefore define an explicit unit-testable scope
   and should not require coverage of compiled-binary entry points or
   process-boundary adapters merely to satisfy a numerical target.

## Active Development Plan

1. **PR21: `test(coverage): define unit coverage boundary` (approximately
   150-300 lines).**
   Establish the precise files and behaviours that belong to the final unit
   coverage target. Compiled-binary execution, fake-`git` PATH tests, and
   process-boundary adapters should remain outside the strict unit gate unless
   they are refactored behind ordinary injectable interfaces.
2. **PR22: `refactor(cli): split parsing and runtime configuration`
   (approximately 500-800 lines).**
   Split the remaining large CLI module by concern, preserving behaviour while
   separating argument parsing, configuration loading, runtime defaults, and
   effective option construction. Move or adjust existing CLI unit tests as
   needed, but avoid expanding behavioural scope beyond coverage-neutral
   refactoring.
3. **PR23: `refactor(render-tests): organise render unit tests by concern`
   (approximately 300-600 lines).**
   Split the large render unit-test file into concern-specific child modules
   while keeping the tests inside the `render` module tree. In Rust, small unit
   suites conventionally live in an inline `#[cfg(test)] mod tests`; larger
   suites may use `#[cfg(test)] mod tests;` plus `tests.rs`, and that file may
   in turn declare child modules such as `tests::metadata`, `tests::orphan`, or
   `tests::trunk`. Top-level `tests/` files are integration tests and should be
   reserved for public API or binary-surface behaviour, not private rendering
   helpers.
4. **PR24: `test(coverage): raise unit coverage to final target`
   (approximately 800-1,500 lines).**
   Add the remaining unit coverage within the boundary established by PR21,
   with coverage ownership following the production module boundaries. This PR
   should be last so that agents are not adding tests against modules that are
   still being moved.

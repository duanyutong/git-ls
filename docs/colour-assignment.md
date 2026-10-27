# Colour assignment

Stack colours identify uninterrupted ancestry segments within the graph.
Each segment retains its colour across branch labels, markers, active lane continuations and ancestry connections.
A junction begins a distinct shared downstack segment; identical shared paths are rendered and weighted once.

## Built-in palette

The application provides one fixed ANSI 256-colour palette.
The following entries specify nominal colour-cube values; terminal rendering, font weight and display characteristics may affect their appearance.

| Colour | ANSI index | Nominal RGB | Family |
| --- | ---: | --- | --- |
| Green | 47 | `#00FF5F` | Green |
| Coral | 203 | `#FF5F5F` | Coral |
| Sky | 39 | `#00AFFF` | Blue |
| Gold | 220 | `#FFD700` | Warm |
| Orchid | 177 | `#D787FF` | Purple |
| Blue | 33 | `#0087FF` | Blue |
| Orange | 208 | `#FF8700` | Warm |

Main-history labels and their spine retain green independently of stack assignment.
Orphaned histories retain their separate grey styling and do not consume assignment entries or exposure budgets.
Added and deleted line counts retain the terminal's standard ANSI green and red.

## Assignment policy

The policy is implemented in `src/render/colours/assignment.rs`, with topology extraction in its `topology` submodule.
Assignment operates on the displayed named ancestry and requires no repository commands, terminal queries or persistent state.

Each segment is weighted by its displayed ancestry-row count, including rewritten-commit rows.
Commit counts, LOC, metadata, object identifiers, checkout state, timestamps, layout and terminal width do not contribute to the assignment objective.
Branch names provide canonical identities for deterministic size ties and colour preferences; segments without branch names use structural order to resolve remaining ties.

Visual neighbours comprise consecutive segment blocks, neighbouring active lanes and child segments meeting a shared parent at a junction.
The graph includes adjacency between consecutive connected groups, whilst orphaned groups are excluded from the calculation.
Sky and blue share one family, as do gold and orange, so different numbered colours within either pair do not constitute sufficient hue separation.

Segments receive an initial assignment in descending weight order, followed by exactly two local repair passes in the same order.
Candidate colours are compared using the following lexicographic priorities:

1. Minimise identical-colour adjacency, weighted by the smaller neighbouring segment.
2. Minimise adjacency within a hue family using the same weights.
3. Minimise excess exposure within either coral or green above a separate target of one fifth of ancestry rows per hue, rounded down.
4. Minimise nearby reuse of the same colour within six segment positions in render order.
5. Minimise concentration of row exposure within each hue family.
6. Minimise concentration of row exposure within each individual colour.
7. Resolve remaining ties using a stable preference derived from the segment's canonical branch name.

The reuse window spans one seven-colour palette cycle and uses each segment's first appearance in render order, including across connected groups.
Each repeated pair contributes the smaller segment weight multiplied by seven minus their positional distance; nearby repetitions therefore carry a greater penalty.
This preference favours unused colours over finer exposure balancing, whilst preserving immediate hue separation and the separate coral and green exposure targets.
Exposure concentration is measured by the incremental change in squared row totals.
Coral and green have independent allowances; using green does not reduce the allowance for coral, or vice versa.
Family exposure balancing favours a missing hue over a second shade of an already prominent blue or warm family when the preceding priorities permit either choice.
The coral and green exposure preferences are soft design targets rather than accessibility thresholds.
Adjacency therefore takes precedence when satisfying both criteria is impossible.
All seven colours remain available, but a small selection need not use every colour before reusing a sufficiently separated alternative.
Each local repair is non-increasing in the same objective; the number of repair passes is bounded independently of repository size.

## Determinism and limitations

The same named topology produces the same assignment across invocations and platforms.
Checking out another branch, changing metadata verbosity, toggling diff statistics, changing layout or resizing the terminal preserves the assignment.
Rewriting object identifiers preserves colours where the displayed named topology is unchanged.
Adding, removing, renaming or restructuring branches may change colours because the weights, identities or adjacency constraints may change.

The heuristic provides no guarantee of a globally optimal colouring or satisfaction of incompatible adjacency constraints.
Where all preferred families are occupied by neighbours, family separation may be relaxed; identical-colour adjacency receives the highest penalty.
A single large stack necessarily occupies a substantial area of one colour because segment continuity is preserved.
The policy preferentially assigns such segments colours outside the coral and green families, reducing competition with diff statistics.

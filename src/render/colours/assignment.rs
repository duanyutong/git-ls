//! Deterministic colour assignment for the built-in seven-colour palette.
//!
//! Each uninterrupted ancestry segment retains one colour; forks and joins delimit segments.
//! Segment weight is its ancestry-row count plus the display width of its branch names, with shared rows counted once.
//! LOC, commit counts, metadata, terminal width, checkout state and timestamps are excluded.
//! Main history retains palette entry zero; orphaned histories do not enter this assignment.
//!
//! Visual neighbours comprise consecutive segment blocks, neighbouring active lanes and segments meeting at a junction.
//! Sky and blue form one hue family; gold and orange form another.
//! Orchid, coral and green each form separate families.
//! Larger segments receive colours first, with canonical branch names breaking size ties.
//! Candidate costs are ordered lexicographically: identical neighbours, related-hue neighbours, excess exposure within either coral or green, nearby colour reuse, family exposure, individual-colour exposure and a fixed palette preference.
//! Equal candidates prefer sky, gold, orchid, blue, orange, coral and green, in that order.
//! Sky precedes blue and gold precedes orange within their respective families; balance and separation take precedence over these shade preferences.
//! Coral and green each have a separate budget of one fifth of the coloured footprint, rounded down.
//! Neither hue consumes the other's allowance; family balancing favours a missing hue over another shade of an already-used family.
//! These are soft design targets: adjacency takes precedence when the constraints conflict.
//! Reuse within six segment positions in render order is penalised by proximity and the smaller segment weight.
//! This palette-sized window favours unused colours before finer exposure balancing, without increasing coral/green dominance or placing related hues beside one another.
//! Squared exposure costs discourage concentrated use and account for both blue shades together.
//!
//! A greedy pass is followed by exactly two local repair passes using the same objective.
//! Every repair is non-increasing in the objective; no random state or persisted assignment is required.
//! Afterwards, shades within each family are relabelled consistently so the largest segment receives the preferred shade.
//! Equal sizes retain canonical processing order; relabelling preserves all six separation and exposure costs.
//! The result is repeatable for the same named topology, although topology changes may recolour segments.
//! The heuristic does not promise a globally optimal colouring or satisfaction of incompatible constraints.

use crate::model::LaneGroup;

mod topology;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Family {
    Blue,
    Warm,
    Purple,
    Coral,
    Green,
}

struct PaletteEntry {
    ansi: u8,
    family: Family,
    preference: usize,
}

const PALETTE: [PaletteEntry; 7] = [
    PaletteEntry {
        ansi: 47,
        family: Family::Green,
        preference: 6,
    },
    PaletteEntry {
        ansi: 203,
        family: Family::Coral,
        preference: 5,
    },
    PaletteEntry {
        ansi: 39,
        family: Family::Blue,
        preference: 0,
    },
    PaletteEntry {
        ansi: 220,
        family: Family::Warm,
        preference: 1,
    },
    PaletteEntry {
        ansi: 177,
        family: Family::Purple,
        preference: 2,
    },
    PaletteEntry {
        ansi: 33,
        family: Family::Blue,
        preference: 3,
    },
    PaletteEntry {
        ansi: 208,
        family: Family::Warm,
        preference: 4,
    },
];

struct Segment {
    weight: usize,
    identity: String,
    neighbours: Vec<usize>,
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Cost {
    identical: usize,
    related: usize,
    semantic_excess: u128,
    nearby_reuse: usize,
    family_exposure: u128,
    colour_exposure: u128,
    preference: usize,
}

pub(super) const fn ansi_colour(index: usize) -> u8 {
    PALETTE[index % PALETTE.len()].ansi
}

/// Return main's fixed entry followed by one palette index per segment in render order.
pub(super) fn assign(groups: &[LaneGroup]) -> Vec<usize> {
    assign_prepared(groups, None)
}

pub(super) fn assign_prepared(
    groups: &[LaneGroup],
    prepared: Option<&crate::render::PreparedTopology>,
) -> Vec<usize> {
    let segments = prepared.map_or_else(
        || topology::segments(groups),
        |prepared| topology::segments_prepared(groups, Some(prepared)),
    );
    let mut order: Vec<_> = (0..segments.len()).collect();
    order.sort_unstable_by(|&left, &right| {
        segments[right]
            .weight
            .cmp(&segments[left].weight)
            .then_with(|| segments[left].identity.cmp(&segments[right].identity))
            .then_with(|| left.cmp(&right))
    });
    let mut assignment = Assignment::new(&segments);
    // The initial greedy pass and two repair passes share the same state transition.
    for _ in 0..3 {
        for &segment in &order {
            let colour = assignment.best_colour(segment);
            assignment.place(segment, colour);
        }
    }
    let mut colours: Vec<_> = assignment
        .colours
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect();
    prefer_readable_shades(&mut colours, &order);
    std::iter::once(0).chain(colours).collect()
}

/// Orient each shade pair without changing colour equality or family membership.
fn prefer_readable_shades(colours: &mut [usize], order: &[usize]) {
    for (preferred, entry) in PALETTE.iter().enumerate() {
        for (alternate, other) in PALETTE.iter().enumerate() {
            if entry.family != other.family || entry.preference >= other.preference {
                continue;
            }
            let first = order
                .iter()
                .map(|&segment| colours[segment])
                .find(|&colour| colour == preferred || colour == alternate);
            if first == Some(alternate) {
                for colour in colours.iter_mut() {
                    if *colour == preferred {
                        *colour = alternate;
                    } else if *colour == alternate {
                        *colour = preferred;
                    }
                }
            }
        }
    }
}

struct Assignment<'a> {
    segments: &'a [Segment],
    colours: Vec<Option<usize>>,
    colour_load: [usize; PALETTE.len()],
    family_load: [usize; 5],
    semantic_family_budget: usize,
}

impl<'a> Assignment<'a> {
    fn new(segments: &'a [Segment]) -> Self {
        Self {
            segments,
            colours: vec![None; segments.len()],
            colour_load: [0; PALETTE.len()],
            family_load: [0; 5],
            semantic_family_budget: segments.iter().map(|segment| segment.weight).sum::<usize>()
                / 5,
        }
    }

    fn best_colour(&self, segment: usize) -> usize {
        let mut best = 0;
        let mut cost = self.cost(segment, best);
        for colour in 1..PALETTE.len() {
            let candidate = self.cost(segment, colour);
            if candidate < cost {
                best = colour;
                cost = candidate;
            }
        }
        best
    }

    fn cost(&self, segment: usize, colour: usize) -> Cost {
        let nearby_reuse = self.reuse_cost(segment, colour);
        let previous = self.colours[segment];
        let previous_family = previous.map(|colour| PALETTE[colour].family);
        let segment = &self.segments[segment];
        let family = PALETTE[colour].family;
        let mut identical = 0;
        let mut related = 0;
        for &neighbour in &segment.neighbours {
            if let Some(other) = self.colours[neighbour] {
                let weight = segment.weight.min(self.segments[neighbour].weight);
                identical += usize::from(other == colour) * weight;
                related += usize::from(PALETTE[other].family == family) * weight;
            }
        }
        // Score replacements against loads with this segment's existing assignment removed.
        let family_load = self.family_load[family as usize]
            - usize::from(previous_family == Some(family)) * segment.weight;
        let colour_load =
            self.colour_load[colour] - usize::from(previous == Some(colour)) * segment.weight;
        let semantic_excess = if is_semantic(family) {
            let excess = family_load.saturating_sub(self.semantic_family_budget);
            let projected_excess =
                (family_load + segment.weight).saturating_sub(self.semantic_family_budget);
            square(projected_excess) - square(excess)
        } else {
            0
        };
        Cost {
            identical,
            related,
            semantic_excess,
            nearby_reuse,
            family_exposure: exposure_cost(family_load, segment.weight),
            colour_exposure: exposure_cost(colour_load, segment.weight),
            preference: PALETTE[colour].preference,
        }
    }

    fn reuse_cost(&self, segment: usize, colour: usize) -> usize {
        self.colours
            .iter()
            .enumerate()
            .take(segment.saturating_add(PALETTE.len()))
            .skip(segment.saturating_sub(PALETTE.len() - 1))
            .filter(|&(other, &assigned)| other != segment && assigned == Some(colour))
            .map(|(other, _)| {
                let proximity = PALETTE.len() - segment.abs_diff(other);
                let weight = self.segments[segment]
                    .weight
                    .min(self.segments[other].weight);
                proximity * weight
            })
            .sum()
    }

    fn place(&mut self, segment: usize, colour: usize) {
        let weight = self.segments[segment].weight;
        let family = PALETTE[colour].family;
        if let Some(previous) = self.colours[segment].replace(colour) {
            let previous_family = PALETTE[previous].family;
            self.colour_load[previous] -= weight;
            self.family_load[previous_family as usize] -= weight;
        }
        self.colour_load[colour] += weight;
        self.family_load[family as usize] += weight;
    }
}

const fn is_semantic(family: Family) -> bool {
    matches!(family, Family::Coral | Family::Green)
}

const fn square(value: usize) -> u128 {
    (value as u128) * (value as u128)
}

const fn exposure_cost(load: usize, weight: usize) -> u128 {
    square(load + weight) - square(load)
}

#[cfg(test)]
#[path = "tests/assignment.rs"]
mod tests;

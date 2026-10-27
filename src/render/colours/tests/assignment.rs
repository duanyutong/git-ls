use super::*;
use crate::model::{BranchAnnotation, BranchPoint, CommitMeta, Lane, RewrittenCommit};

fn lane(names: &[&str]) -> Lane {
    Lane::new_with_rewritten_commits(
        names.first().copied().unwrap_or("empty"),
        Some("main".to_string()),
        names
            .iter()
            .map(|name| BranchPoint::new(*name, [*name], None))
            .collect(),
        Vec::new(),
        0,
        false,
    )
}

fn group(lanes: Vec<Lane>) -> LaneGroup {
    LaneGroup::new(Some("main".to_string()), None, Some(0), lanes)
}

fn weighted_lane(prefix: &str, weight: usize) -> Lane {
    let names: Vec<_> = (0..weight).map(|i| format!("{prefix}/{i:03}")).collect();
    lane(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

fn assert_neighbours_differ(segments: &[Segment], colours: &[usize]) {
    for (index, segment) in segments.iter().enumerate() {
        for &neighbour in &segment.neighbours {
            assert_ne!(colours[index], colours[neighbour]);
            assert!(PALETTE[colours[index]].1 != PALETTE[colours[neighbour]].1);
        }
    }
}

#[test]
fn built_in_palette_contains_the_seven_selected_colours() {
    assert_eq!(
        (0..7).map(ansi_colour).collect::<Vec<_>>(),
        [47, 203, 39, 220, 177, 33, 208]
    );
    assert_eq!(ansi_colour(7), 47);
}

#[test]
fn empty_and_orphaned_histories_leave_only_the_fixed_main_colour() {
    assert_eq!(assign(&[]), [0]);
    assert_eq!(assign(&[group(Vec::new())]), [0]);
    let mut orphan = group(vec![weighted_lane("orphan", 100)]);
    orphan.main_distance = None;
    assert_eq!(assign(&[orphan.clone()]), [0]);
    let groups = [
        group(vec![weighted_lane("first", 15)]),
        group(vec![lane(&["last"])]),
    ];
    assert_eq!(
        assign(&groups),
        assign(&[groups[0].clone(), orphan, groups[1].clone()])
    );
}

#[test]
fn large_stacks_avoid_coral_and_green_independently_of_display_order() {
    for weights in [[15, 10, 3, 1], [1, 3, 10, 15], [10, 1, 15, 3]] {
        let groups = [group(
            weights
                .iter()
                .enumerate()
                .map(|(i, &weight)| weighted_lane(&format!("stack-{i}"), weight))
                .collect(),
        )];
        let segments = topology::segments(&groups);
        let colours = assign(&groups);
        let colours = &colours[1..];
        let mut semantic_rows = 0;
        for (segment, &colour) in segments.iter().zip(colours) {
            if is_semantic(PALETTE[colour].1) {
                assert!(segment.weight <= 3);
                semantic_rows += segment.weight;
            }
        }
        assert!(semantic_rows <= 29 / 5);
        assert_neighbours_differ(&segments, colours);
    }
    for count in 1..=2 {
        let groups = [group(
            (0..count)
                .map(|i| weighted_lane(&i.to_string(), 10))
                .collect(),
        )];
        assert!(
            assign(&groups)[1..]
                .iter()
                .all(|&colour| !is_semantic(PALETTE[colour].1))
        );
    }
}

#[test]
fn shared_paths_are_weighted_once_and_every_junction_gets_distinct_colours() {
    let groups = [group(vec![
        lane(&["alpha", "inner", "shared", "base"]),
        lane(&["independent"]),
        lane(&["beta", "inner", "shared", "base"]),
        lane(&["gamma", "shared", "base"]),
    ])];
    let segments = topology::segments(&groups);
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.weight)
            .collect::<Vec<_>>(),
        [1, 1, 1, 1, 2, 1]
    );
    assert_eq!(
        segments.iter().map(|segment| segment.weight).sum::<usize>(),
        7
    );
    assert_neighbours_differ(&segments, &assign(&groups)[1..]);
}

#[test]
fn persistent_lanes_and_consecutive_groups_are_visual_neighbours() {
    let groups = [
        group(vec![
            lane(&["left"]),
            lane(&["alpha", "base"]),
            lane(&["beta", "base"]),
        ]),
        group(vec![lane(&["historical"])]),
    ];
    let segments = topology::segments(&groups);
    // The left lane remains beside the parent after the fork closes.
    assert!(segments[0].neighbours.contains(&3));
    // The final segment in the first group borders the first segment in the second.
    assert!(segments[3].neighbours.contains(&4));
    assert_neighbours_differ(&segments, &assign(&groups)[1..]);
}

#[test]
fn metadata_checkout_alias_order_and_rebased_object_ids_do_not_change_assignment() {
    let groups = [group(vec![
        lane(&["alpha", "shared"]),
        lane(&["beta", "shared"]),
    ])];
    let mut changed = groups.clone();
    for lane in &mut changed[0].lanes {
        lane.contains_current = true;
        lane.head_oid = format!("rebased-{}", lane.head_oid);
        lane.head_timestamp = 999;
        for point in &mut lane.branch_points {
            point.oid = format!("rebased-{}", point.oid);
            point.commit_count = 100;
            point.annotation = Some(BranchAnnotation::new(
                CommitMeta::new(&point.oid, 999, "subject"),
                100,
            ));
            point.names.push(format!("z-alias/{}", point.names[0]));
            point.names.reverse();
        }
    }
    assert_eq!(assign(&groups), assign(&changed));
    assert_eq!(assign(&changed), assign(&changed));
}

#[test]
fn duplicate_paths_and_empty_lanes_do_not_consume_colours_or_weight() {
    let first = lane(&["alpha", "shared"]);
    let second = lane(&["beta", "shared"]);
    assert_eq!(
        assign(&[group(vec![first.clone(), second.clone()])]),
        assign(&[group(vec![lane(&[]), first.clone(), first, second])])
    );
}

#[test]
fn rewritten_only_segments_and_distinct_replacements_remain_deterministic() {
    let mut first = lane(&["topic"]);
    first.rewritten_commits.push(RewrittenCommit::new(
        CommitMeta::new("old", 0, "old"),
        CommitMeta::new("new", 0, "new"),
    ));
    let mut second = first.clone();
    second.rewritten_commits[0].replacement = CommitMeta::new("other", 0, "other");
    let mut rewritten_only = lane(&[]);
    rewritten_only
        .rewritten_commits
        .clone_from(&first.rewritten_commits);
    let mut groups = [group(vec![first, second, rewritten_only])];
    let duplicate_segments = topology::segments(&groups);
    assert_eq!(duplicate_segments.len(), 2);
    assert_eq!(
        duplicate_segments
            .iter()
            .map(|segment| segment.weight)
            .sum::<usize>(),
        4
    );
    groups[0].lanes[2].rewritten_commits[0].replacement = CommitMeta::new("third", 0, "third");
    let segments = topology::segments(&groups);
    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments.iter().map(|segment| segment.weight).sum::<usize>(),
        5
    );
    assert_neighbours_differ(&segments, &assign(&groups)[1..]);
}

#[test]
fn small_stacks_use_all_hue_families_without_repetition_or_semantic_dominance() {
    let groups = [
        group(vec![
            lane(&["fix/payment-retries", "feat/invoices", "feat/checkout"]),
            lane(&["perf/search-ranking", "refactor/search-index"]),
            lane(&["test/report-export"]),
            lane(&["ci/import-checks"]),
        ]),
        group(vec![
            lane(&["docs/configuration", "docs/quickstart"]),
            lane(&["fix/cache-expiry", "chore/cache-keys"]),
        ]),
    ];
    let segments = topology::segments(&groups);
    let colours = assign(&groups);
    let colours = &colours[1..];
    let unique: std::collections::BTreeSet<_> = colours.iter().collect();
    assert_eq!(unique.len(), segments.len());
    assert_neighbours_differ(&segments, colours);
    for family in [
        Family::Blue,
        Family::Warm,
        Family::Purple,
        Family::Coral,
        Family::Green,
    ] {
        assert!(colours.iter().any(|&colour| PALETTE[colour].1 == family));
        if is_semantic(family) {
            let rows: usize = segments
                .iter()
                .zip(colours)
                .filter(|(_, colour)| PALETTE[**colour].1 == family)
                .map(|(segment, _)| segment.weight)
                .sum();
            assert!(rows <= 2);
        }
    }
    assert!(!is_semantic(PALETTE[colours[0]].1));
}

#[test]
fn nearby_reuse_spreads_colours_and_expires_after_one_palette_cycle() {
    let segments: Vec<_> = (0..14)
        .map(|i| Segment {
            weight: 1,
            identity: format!("stack-{i}"),
            neighbours: Vec::new(),
        })
        .collect();
    let mut assignment = Assignment::new(&segments);
    for colour in 0..PALETTE.len() {
        assignment.place(colour, colour);
    }
    assignment.place(8, 0); // Fill green's independent row budget.
    assignment.place(9, 1); // Fill coral's independent row budget.

    // Sky is the most distant eligible colour, despite higher blue-family exposure.
    assert_eq!(ansi_colour(assignment.best_colour(7)), 39);
    // Beyond the reuse window, orchid's lower family exposure takes precedence again.
    assert_eq!(ansi_colour(assignment.best_colour(13)), 177);
}

#[test]
fn large_selections_reuse_colours_without_adjacent_hue_repetition() {
    let groups = [group(
        (0..128)
            .map(|i| weighted_lane(&format!("topic-{i}"), 1 + i % 9))
            .collect(),
    )];
    let segments = topology::segments(&groups);
    let colours = assign(&groups);
    assert_neighbours_differ(&segments, &colours[1..]);
    for colour in 0..PALETTE.len() {
        assert!(colours[1..].contains(&colour));
    }
}

#[test]
fn family_and_individual_exposure_have_separate_costs() {
    let segments = [
        Segment {
            weight: 10,
            identity: "large".to_string(),
            neighbours: Vec::new(),
        },
        Segment {
            weight: 1,
            identity: "small".to_string(),
            neighbours: Vec::new(),
        },
    ];
    let mut assignment = Assignment::new(&segments);
    assignment.place(0, 2); // Sky.
    let sky = assignment.cost(1, 2);
    let blue = assignment.cost(1, 5);
    let gold = assignment.cost(1, 3);
    assert_eq!(sky.family_exposure, blue.family_exposure);
    assert!(blue.colour_exposure < sky.colour_exposure);
    assert!(gold.family_exposure < blue.family_exposure);
}

#[test]
fn repair_reduces_conflicts_and_degrades_gracefully_when_seven_colours_cannot_suffice() {
    let segments: Vec<_> = (0..8)
        .map(|i| Segment {
            weight: i + 1,
            identity: i.to_string(),
            neighbours: (0..8).filter(|&other| other != i).collect(),
        })
        .collect();
    let mut assignment = Assignment::new(&segments);
    for segment in 0..segments.len() {
        assignment.place(segment, 0);
    }
    for segment in 0..segments.len() {
        let best = assignment.best_colour(segment);
        assignment.place(segment, best);
    }
    let unique: std::collections::BTreeSet<_> = assignment.colours.iter().copied().collect();
    assert_eq!(unique.len(), 7);
    assert!(!unique.contains(&None));
    let semantic_rows = assignment.family_load[Family::Coral as usize]
        + assignment.family_load[Family::Green as usize];
    assert!(semantic_rows < segments.iter().map(|segment| segment.weight).sum());
}

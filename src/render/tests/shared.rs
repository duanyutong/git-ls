use crate::cli::{Layout, Verbosity};
use crate::model::{Lane, LaneGroup, RewrittenCommit};
use crate::render::RenderContext;
use crate::render::graph::MainSpine;
use crate::render::layout::{render_group, render_lane_groups};
use crate::render::metadata::{MetadataWidths, calculate_metadata_widths};
use crate::render::trunk::TrunkLabel;
use crate::test_support::TEST_NOW;

use super::{meta, point_with_count, test_colours};

fn lane(names: &[&str]) -> Lane {
    Lane {
        head_oid: names.first().copied().unwrap_or("empty").to_string(),
        base_oid: Some("main".to_string()),
        branch_points: names
            .iter()
            .map(|name| point_with_count(name, &[name], 2, &format!("feat: add {name}")))
            .collect(),
        rewritten_commits: Vec::new(),
        head_timestamp: 1,
        contains_current: false,
    }
}

fn render(lanes: &[Lane]) -> Vec<String> {
    let colours = test_colours(false);
    let ctx = RenderContext::new(
        "main",
        None,
        Some("shared"),
        Some("shared"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );
    render_group(
        lanes,
        lanes.len(),
        &mut 1,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    )
    .iter()
    .map(|line| line.as_str().to_string())
    .collect()
}

#[test]
fn renders_shared_downstack_once_below_child_junction() {
    assert_eq!(
        render(&[
            lane(&["search", "shared", "base"]),
            lane(&["export", "shared", "base"])
        ]),
        [
            "    ◯   search",
            "    │ ◯ export",
            "    ├─┘",
            "▶   ●   shared",
            "  ⁝ ◯   base",
            "  ◇─┘   main",
        ]
    );
}

#[test]
fn keeps_nested_forks_together_when_head_order_interleaves_families() {
    assert_eq!(
        render(&[
            lane(&["alpha", "inner", "shared"]),
            lane(&["independent"]),
            lane(&["beta", "inner", "shared"]),
            lane(&["gamma", "shared"]),
        ]),
        [
            "    ◯       alpha",
            "    │   ◯   beta",
            "    ├───┘  ",
            "    ◯       inner",
            "    │     ◯ gamma",
            "    ├─────┘",
            "▶   ●       shared",
            "  ⁝ │ ◯     independent",
            "  ◇─┴─┘     main",
        ]
    );
}

#[test]
fn connects_only_surviving_roots_to_main_across_closed_lanes() {
    assert_eq!(
        render(&[
            lane(&["alpha", "shared"]),
            lane(&["beta", "shared"]),
            lane(&["independent"]),
        ]),
        [
            "    ◯     alpha",
            "    │ ◯   beta",
            "    ├─┘  ",
            "▶   ●     shared",
            "  ⁝ │   ◯ independent",
            "  ◇─┴───┘ main",
        ]
    );
}

#[test]
fn aligns_historical_forks_with_wider_groups() {
    let colours = test_colours(false);
    let ctx = RenderContext::new(
        "main",
        None,
        Some("shared"),
        Some("shared"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );
    let base = meta("old-main", "old base");
    let output = render_group(
        &[lane(&["alpha", "shared"]), lane(&["beta", "shared"])],
        3,
        &mut 1,
        &ctx,
        TrunkLabel::Commit(&base),
        MainSpine::Connected,
    );
    assert_eq!(
        output,
        [
            "  │   ◯   alpha",
            "  │   │ ◯ beta",
            "  │   ├─┘",
            "▶ │   ●   shared",
            "  ◇───┘   old base",
        ]
    );
}

#[test]
fn keeps_independent_lanes_visible_beside_a_fork() {
    assert_eq!(
        render(&[
            lane(&["independent"]),
            lane(&["alpha", "shared"]),
            lane(&["beta", "shared"]),
        ]),
        [
            "    ◯     independent",
            "    │ ◯   alpha",
            "    │ │ ◯ beta",
            "    │ ├─┘",
            "▶ ⁝ │ ●   shared",
            "  ◇─┴─┘   main",
        ]
    );
}

#[test]
fn renders_three_children_and_duplicate_head_paths_without_duplicate_rows() {
    assert_eq!(
        render(&[
            lane(&["alpha", "shared"]),
            lane(&["beta", "shared"]),
            lane(&["gamma", "shared"]),
            lane(&["alpha", "shared"]),
        ]),
        [
            "    ◯       alpha",
            "    │ ◯     beta",
            "    │ │ ◯   gamma",
            "    ├─┴─┘  ",
            "▶ ⁝ ●       shared",
            "  ◇─┘       main",
        ]
    );
}

#[test]
fn shares_rewritten_ancestry_and_keeps_replacements_distinct() {
    let mut first = lane(&["alpha", "shared"]);
    first.rewritten_commits = vec![RewrittenCommit::new(
        meta("old-base", "old"),
        meta("new-base", "new"),
    )];
    let mut second = lane(&["beta", "shared"]);
    second
        .rewritten_commits
        .clone_from(&first.rewritten_commits);
    let output = render(&[first.clone(), second.clone()]);
    assert_eq!(
        output.iter().filter(|line| line.contains("shared")).count(),
        1
    );
    assert_eq!(output.iter().filter(|line| line.contains('✕')).count(), 1);
    assert_eq!(output[2], "    ├─┘");
    assert_eq!(output[4], "  ⁝ ✕   old-bas rewritten as new-bas");

    second.rewritten_commits[0].replacement = meta("another-base", "another");
    let output = render(&[first, second]);
    assert_eq!(
        output.iter().filter(|line| line.contains("shared")).count(),
        2
    );
    assert_eq!(output.iter().filter(|line| line.contains('✕')).count(), 2);
    assert!(!output.iter().any(|line| line.contains('├')));
}

#[test]
fn shares_rewritten_junction_below_distinct_branch_paths() {
    let mut first = lane(&["alpha"]);
    first.rewritten_commits = vec![RewrittenCommit::new(
        meta("old-base", "old"),
        meta("new-base", "new"),
    )];
    let mut second = lane(&["beta"]);
    second
        .rewritten_commits
        .clone_from(&first.rewritten_commits);
    assert_eq!(
        render(&[first, second]),
        [
            "    ◯   alpha",
            "    │ ◯ beta",
            "    ├─┘",
            "  ⁝ ✕   old-bas rewritten as new-bas",
            "  ◇─┘   main",
        ]
    );
}

#[test]
fn preserves_metadata_aliases_detached_head_and_colour_in_both_layouts() {
    let colours = test_colours(true);
    let mut first = lane(&["alpha", "shared"]);
    first.branch_points[1].names.push("alias".to_string());
    let mut second = lane(&["beta", "shared"]);
    second.branch_points[1].names.push("alias".to_string());
    let groups = vec![LaneGroup::new(
        Some("old-main".to_string()),
        Some(meta("old-main", "base")),
        Some(3),
        vec![first, second],
    )];
    for layout in [Layout::Inline, Layout::Columns] {
        let ctx = RenderContext::new(
            "main",
            None,
            None,
            Some("shared"),
            TEST_NOW,
            Verbosity::High,
            calculate_metadata_widths(&groups, None, TEST_NOW, Verbosity::High),
            &colours,
        )
        .with_layout(layout);
        let output = render_lane_groups(&groups, &ctx);
        let text = output
            .iter()
            .map(|line| line.as_str().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text.matches("feat: add shared").count(), 1);
        assert_eq!(text.matches("alias").count(), 1);
        assert_eq!(text.matches('◉').count(), 1);
        assert!(!text.contains('▶'));
        let row = output
            .iter()
            .find(|line| line.contains("alias"))
            .expect("shared row");
        let colour = marker_colour(row, '◉');
        for name in ["shared", "alias"] {
            assert!(row.contains(&format!("\x1b[38;5;{colour}m{name}\x1b[0m")));
        }
        let plain = console::strip_ansi_codes(row.as_str());
        let expected = if layout.is_columns() {
            " 2 shared, alias "
        } else {
            "(2, shared) shared, alias "
        };
        assert!(plain.contains(expected), "{plain}");
        let junction = output
            .iter()
            .find(|row| row.contains('├'))
            .expect("junction row");
        let alpha = output
            .iter()
            .find(|row| row.contains("feat: add alpha"))
            .expect("alpha row");
        assert_eq!(marker_colour(junction, '├'), marker_colour(alpha, '◯'));
    }
}

#[test]
fn empty_paths_do_not_create_branch_rows() {
    assert_eq!(render(&[]), ["  ◇── main"]);
}

fn coloured_rows(groups: &[LaneGroup]) -> Vec<crate::render::RenderLine> {
    let colours = crate::render::Colours::new(true);
    let ctx = RenderContext::new(
        "main",
        None,
        Some("shared"),
        Some("shared"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );
    render_lane_groups(groups, &ctx)
}

fn group(lanes: Vec<Lane>, distance: Option<usize>) -> LaneGroup {
    LaneGroup::new(Some("main".to_string()), None, distance, lanes)
}

fn marker_colour(row: &str, marker: char) -> u8 {
    let end = row.find(marker).expect("marker present");
    row[..end]
        .rsplit_once("\x1b[38;5;")
        .expect("256-colour foreground")
        .1
        .trim_end_matches('m')
        .parse()
        .expect("colour index")
}

fn branch_colour(output: &[crate::render::RenderLine], name: &str) -> u8 {
    let row = output
        .iter()
        .find(|line| console::strip_ansi_codes(line).ends_with(name))
        .expect("branch row");
    marker_colour(row, if name == "shared" { '●' } else { '◯' })
}

#[test]
fn preserves_segment_continuity_and_consistent_glyph_colours_at_nested_junctions() {
    let groups = [
        group(
            vec![
                lane(&["alpha", "inner", "shared", "base"]),
                lane(&["independent"]),
                lane(&["beta", "inner", "shared", "base"]),
                lane(&["gamma", "shared", "base"]),
            ],
            Some(0),
        ),
        group(vec![lane(&["orphan"])], None),
        group(vec![lane(&["historical"])], Some(2)),
    ];
    let output = coloured_rows(&groups);
    let shared = branch_colour(&output, "shared");
    let independent = branch_colour(&output, "independent");
    assert_eq!(shared, branch_colour(&output, "base"));
    for (left, right) in [
        ("alpha", "beta"),
        ("alpha", "inner"),
        ("beta", "inner"),
        ("inner", "gamma"),
        ("gamma", "shared"),
        ("inner", "shared"),
        ("shared", "independent"),
        ("independent", "historical"),
    ] {
        assert_ne!(branch_colour(&output, left), branch_colour(&output, right));
    }
    for name in [
        "alpha",
        "beta",
        "inner",
        "gamma",
        "shared",
        "base",
        "independent",
        "historical",
    ] {
        let colour = branch_colour(&output, name);
        let row = output
            .iter()
            .find(|line| console::strip_ansi_codes(line).ends_with(name))
            .unwrap();
        assert!(row.contains(&format!("\x1b[38;5;{colour}m{name}\x1b[0m")));
    }
    let current = output
        .iter()
        .find(|line| line.contains('▶'))
        .expect("current row");
    assert!(current.contains(&format!("\x1b[1m\x1b[38;5;{shared}m▶")));
    assert!(current.contains(&format!("\x1b[1m\x1b[4m\x1b[38;5;{shared}mshared")));
    let independent_row = output
        .iter()
        .find(|line| line.contains("independent"))
        .unwrap();
    assert!(independent_row.contains(&format!("\x1b[38;5;{shared}m│")));
    let main = output
        .iter()
        .find(|line| line.contains('◇'))
        .expect("main row");
    assert!(main.contains(&format!("\x1b[38;5;{shared}m─┴")));
    assert!(main.contains(&format!("\x1b[38;5;{independent}m─┘")));
    assert_eq!(marker_colour(main, '◇'), 47);
}

#[test]
fn reuses_the_built_in_palette_for_large_forks_without_losing_shared_continuity() {
    let count = 32;
    let names: Vec<_> = (0..count).map(|index| format!("topic-{index}")).collect();
    let lanes = names
        .iter()
        .map(|name| lane(&[name, "shared", "base"]))
        .collect();
    let output = coloured_rows(&[group(lanes, Some(0))]);
    let mut previous = None;
    for name in &names {
        let colour = branch_colour(&output, name);
        assert!([47, 203, 39, 220, 177, 33, 208].contains(&colour));
        assert_ne!(previous, Some(colour));
        previous = Some(colour);
    }
    assert_eq!(
        branch_colour(&output, "shared"),
        branch_colour(&output, "base")
    );
}

#[test]
fn duplicate_paths_do_not_change_segment_colours() {
    let original = coloured_rows(&[group(
        vec![lane(&["alpha", "shared"]), lane(&["beta", "shared"])],
        Some(0),
    )]);
    let duplicated = coloured_rows(&[group(
        vec![
            lane(&["alpha", "shared"]),
            lane(&["alpha", "shared"]),
            lane(&["beta", "shared"]),
        ],
        Some(0),
    )]);
    for name in ["alpha", "beta", "shared"] {
        assert_eq!(
            branch_colour(&original, name),
            branch_colour(&duplicated, name)
        );
    }
}

#[test]
fn display_settings_and_checkout_state_preserve_colours_and_plain_graph_shape() {
    let groups = [group(
        vec![
            lane(&["alpha", "shared", "base"]),
            lane(&["beta", "shared", "base"]),
        ],
        Some(0),
    )];
    let colours = test_colours(true);
    let mut expected = None;
    for layout in [Layout::Inline, Layout::Columns] {
        for verbosity in [Verbosity::Low, Verbosity::Medium, Verbosity::High] {
            for current in [Some("alpha"), Some("shared"), None] {
                let ctx = RenderContext::new(
                    "main",
                    None,
                    current,
                    Some("shared"),
                    TEST_NOW + 500,
                    verbosity,
                    MetadataWidths::default(),
                    &colours,
                )
                .with_layout(layout);
                let output = render_lane_groups(&groups, &ctx);
                let assigned: Vec<_> = output
                    .iter()
                    .filter_map(|row| {
                        row.chars()
                            .find(|c| matches!(c, '●' | '◯' | '◉'))
                            .map(|marker| marker_colour(row, marker))
                    })
                    .collect();
                if let Some(expected) = &expected {
                    assert_eq!(&assigned, expected);
                } else {
                    expected = Some(assigned);
                }
                let plain_colours = test_colours(false);
                let plain_ctx = RenderContext {
                    colours: &plain_colours,
                    ..ctx
                };
                let plain = render_lane_groups(&groups, &plain_ctx);
                assert_eq!(
                    output
                        .iter()
                        .map(|row| console::strip_ansi_codes(row).into_owned())
                        .collect::<Vec<_>>(),
                    plain
                        .iter()
                        .map(|row| row.as_str().to_string())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn prepared_clipped_graphs_preserve_visible_cells_and_avoid_hidden_colour_sequences() {
    use crate::render::PreparedTopology;
    use crate::terminal::{RenderEnvironment, write_rendered_line};
    let lanes: Vec<_> = (0..128)
        .map(|index| lane(&[&format!("topic-{index}"), "shared"]))
        .collect();
    let mut groups = vec![LaneGroup::new(Some("main".into()), None, Some(0), lanes)];
    groups[0].lanes[0]
        .rewritten_commits
        .push(RewrittenCommit::new(meta("old", "old"), meta("new", "new")));
    let prepared = PreparedTopology::new(&groups);
    let colours = test_colours(true).prepare(&groups, &prepared);
    let full = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    )
    .with_layout(Layout::Columns);
    let clipped = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    )
    .with_layout(Layout::Columns)
    .with_topology(&prepared, Some(40));
    let environment = RenderEnvironment::new(TEST_NOW, Some(40), true);
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in render_lane_groups(&groups, &full) {
        write_rendered_line(&mut expected, &line, environment).unwrap();
    }
    for line in render_lane_groups(&groups, &clipped) {
        write_rendered_line(&mut actual, &line, environment).unwrap();
    }
    let expected = String::from_utf8(expected).unwrap();
    let actual = String::from_utf8(actual).unwrap();
    assert_eq!(
        console::strip_ansi_codes(&actual),
        console::strip_ansi_codes(&expected)
    );
    assert!(actual.len() < 100_000, "{}", actual.len());
    // A prepared layout remains safe when called with a different lane structure.
    let empty: Vec<LaneGroup> = vec![LaneGroup::new(
        Some("main".into()),
        None,
        Some(0),
        vec![lane(&["different"])],
    )];
    assert_ne!(
        render_lane_groups(&empty, &clipped),
        Vec::<crate::render::RenderLine>::new()
    );
}

#[test]
fn stale_prepared_rows_fall_back_when_lanes_or_rewrite_positions_disappear() {
    use crate::render::PreparedTopology;
    let mut original = lane(&["point"]);
    original
        .rewritten_commits
        .push(RewrittenCommit::new(meta("old", "old"), meta("new", "new")));
    let groups = vec![LaneGroup::new(None, None, Some(0), vec![original])];
    let prepared = PreparedTopology::new(&groups);
    let colours = test_colours(false);
    let context = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    )
    .with_topology(&prepared, Some(40));
    for lanes in [Vec::new(), vec![lane(&["point"])]] {
        let changed = vec![LaneGroup::new(None, None, Some(0), lanes)];
        assert_ne!(
            render_lane_groups(&changed, &context),
            Vec::<crate::render::RenderLine>::new()
        );
    }
}

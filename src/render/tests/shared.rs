use crate::cli::{Layout, Palette, Verbosity};
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
        assert!(row.contains("\x1b[38;5;220m◉\x1b[0m"));
        let plain = console::strip_ansi_codes(row.as_str());
        let expected = if layout.is_columns() {
            " 2 shared, alias "
        } else {
            "(2, shared) shared, alias "
        };
        assert!(plain.contains(expected), "{plain}");
        assert!(text.contains("\x1b[38;5;203m├\x1b[0m"));
    }
}

#[test]
fn empty_paths_do_not_create_branch_rows() {
    assert_eq!(render(&[]), ["  ◇── main"]);
}

fn coloured_rows(groups: &[LaneGroup], palette: Palette) -> Vec<crate::render::RenderLine> {
    let colours = crate::render::Colours::new(true, palette);
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

#[test]
fn rotates_in_display_order_at_nested_junctions_and_across_groups() {
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
    let output = coloured_rows(&groups, Palette::Classic);
    let colours = test_colours(true);
    for (name, colour_index) in [
        ("alpha", 1),
        ("beta", 2),
        ("inner", 3),
        ("gamma", 4),
        ("shared", 5),
        ("base", 5),
        ("independent", 6),
        ("historical", 7),
    ] {
        let row = output
            .iter()
            .find(|line| console::strip_ansi_codes(line).ends_with(name))
            .expect("branch row");
        let marker = if name == "shared" { "●" } else { "◯" };
        assert!(
            row.contains(&colours.stack(colour_index, marker)),
            "{name}: {row:?}"
        );
        let label = if name == "shared" {
            colours.current_stack(colour_index, name)
        } else {
            colours.stack(colour_index, name)
        };
        assert!(row.contains(&label), "{name}: {row:?}");
    }
    let current = output
        .iter()
        .find(|line| line.contains('▶'))
        .expect("current row");
    assert!(current.contains(&colours.current_indicator(5, "▶")));
    let independent = output
        .iter()
        .find(|line| line.contains("independent"))
        .expect("independent row");
    assert!(independent.contains(&colours.stack(5, "│")));
    let main = output
        .iter()
        .find(|line| line.contains('◇'))
        .expect("main row");
    assert!(main.contains(&colours.stack(5, "─┴")));
    assert!(main.contains(&colours.stack(6, "─┘")));
    assert!(main.contains(&colours.stack(0, "◇")));
}

#[test]
fn rotates_shared_segments_until_the_selected_palette_is_exhausted() {
    for palette in [Palette::Classic, Palette::Paired] {
        let count = palette.ansi_colours().len() + 2;
        let names: Vec<_> = (0..count).map(|index| format!("topic-{index}")).collect();
        let lanes = names
            .iter()
            .map(|name| lane(&[name, "shared", "base"]))
            .collect();
        let output = coloured_rows(&[group(lanes, Some(0))], palette);
        for (name, index) in names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index + 1))
            .chain([("shared", count + 1), ("base", count + 1)])
        {
            let row = output
                .iter()
                .find(|line| console::strip_ansi_codes(line).ends_with(name))
                .expect("branch row");
            let marker = if name == "shared" { "●" } else { "◯" };
            let code = palette.ansi_colours()[index % palette.ansi_colours().len()];
            assert!(
                row.contains(&format!("\x1b[38;5;{code}m{marker}\x1b[0m")),
                "{name}: {row:?}"
            );
        }
    }
}

#[test]
fn duplicate_paths_do_not_skip_unused_colours() {
    let groups = [group(
        vec![
            lane(&["alpha", "shared"]),
            lane(&["alpha", "shared"]),
            lane(&["beta", "shared"]),
        ],
        Some(0),
    )];
    let output = coloured_rows(&groups, Palette::Classic);
    let colours = test_colours(true);
    let beta = output
        .iter()
        .find(|line| line.contains("beta"))
        .expect("beta row");
    assert!(beta.contains(&colours.stack(2, "◯")));
    let shared = output
        .iter()
        .find(|line| line.contains("shared"))
        .expect("shared row");
    assert!(shared.contains(&colours.stack(3, "●")));
}

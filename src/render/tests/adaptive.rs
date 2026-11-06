use crate::cli::{Layout, Verbosity};
use crate::model::{DiffStat, Lane, LaneGroup};
use crate::render::branch::BranchLabel;
use crate::render::metadata::MetadataWidths;
use crate::render::{
    PreparedTopology, RenderContext, RenderLine, calculate_metadata_widths, render_lane_groups,
};
use crate::terminal::{RenderEnvironment, write_rendered_line};
use crate::test_support::TEST_NOW;

use super::{point, point_with_count, test_colours};

fn output(lines: &[RenderLine], width: Option<usize>) -> Vec<String> {
    let mut bytes = Vec::new();
    for line in lines {
        write_rendered_line(
            &mut bytes,
            line,
            RenderEnvironment::new(TEST_NOW, width, true),
        )
        .unwrap();
    }
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn spends_width_on_names_and_statistics_before_subjects() {
    let colours = test_colours(false);
    let mut point = point_with_count("abcdef0", &["topic"], 2, "a rather long subject");
    point.diff_stat = Some(DiffStat {
        added: 12,
        deleted: 3,
    });
    let ctx = RenderContext::new(
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
    let label = BranchLabel::new(&point, 0, false, &ctx);
    for (width, expected) in [
        (
            Some(46),
            vec!["● 2 topic a rather long subject +12 -3 abcdef0"],
        ),
        (Some(32), vec!["● 2 topic a ra... +12 -3 abcdef0"]),
        (Some(27), vec!["● 2 topic    +12 -3 abcdef0"]),
        (Some(24), vec!["● 2 topic +12 -3 abcdef0"]),
        (Some(23), vec!["● 2 topic", "  │      +12 -3 abcdef0"]),
    ] {
        let actual = output(&label.render("● ", &|| "  │".to_string(), width), width);
        // Alignment depends on the complete terminal width, including whitespace.
        assert_eq!(actual, expected, "{width:?}");
        assert!(actual[0].contains("topic"));
        assert!(actual.last().unwrap().ends_with("+12 -3 abcdef0"));
        assert_eq!(
            console::measure_text_width(actual.last().unwrap()),
            width.unwrap()
        );
    }
    let single = label.render("● ", &String::new, None);
    assert_eq!(single.len(), 1);
    assert!(single[0].contains("a rather long subject"));
    let without_subject = output(&label.render("● ", &String::new, Some(24)), Some(24));
    assert!(!without_subject[0].contains("..."));
    assert!(!without_subject[0].contains("subject"));
}

#[test]
fn compact_names_remain_on_one_line_and_retain_the_checked_out_branch() {
    let colours = test_colours(true);
    for (names, current, width, expected) in [
        (vec!["a"], None, 18, "a"),
        (vec!["a", "b"], None, 18, "a, b"),
        (vec!["a", "b", "c"], None, 18, "a, b, c"),
        (vec!["a", "b", "c", "d"], None, 18, "a, b, c, ..."),
        (vec!["a", "b", "c", "d"], Some("d"), 18, "d, a, b, ..."),
        (
            vec!["first", "second", "last"],
            Some("last"),
            17,
            "last, ...",
        ),
        (vec!["longbranch", "other"], None, 9, "long..."),
        (vec!["界界界界界"], None, 9, "界界..."),
        (
            vec!["e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}"],
            None,
            9,
            "e\u{301}e\u{301}e\u{301}e...",
        ),
    ] {
        let mut point = point_with_count("abcdef0", &names, 1, "subject");
        point.diff_stat = Some(DiffStat {
            added: 123,
            deleted: 4,
        });
        let ctx = RenderContext::new(
            "main",
            None,
            current,
            None,
            TEST_NOW,
            Verbosity::High,
            MetadataWidths::default(),
            &colours,
        )
        .with_layout(Layout::Columns);
        let rows = BranchLabel::new(&point, 0, false, &ctx).render("", &String::new, Some(width));
        assert_eq!(rows.len(), 2);
        let first = console::strip_ansi_codes(&rows[0]);
        assert_eq!(first, format!("1 {expected}"));
        assert!(!rows[0].contains("subject"));
        assert_eq!(rows.iter().filter(|row| row.contains("+123")).count(), 1);
        if current == Some("d") {
            assert!(rows[0].contains(&colours.current_stack(0, "d")));
        }
    }
}

#[test]
fn retains_exact_values_at_tiny_widths_and_handles_missing_fields() {
    let colours = test_colours(false);
    let mut point = point_with_count("abcdef0", &["long-branch"], 1, "subject");
    point.diff_stat = Some(DiffStat {
        added: u64::MAX,
        deleted: 0,
    });
    for layout in [Layout::Inline, Layout::Columns] {
        for verbosity in [Verbosity::Low, Verbosity::Medium, Verbosity::High] {
            let ctx = RenderContext::new(
                "main",
                None,
                None,
                None,
                TEST_NOW,
                verbosity,
                MetadataWidths::default(),
                &colours,
            )
            .with_layout(layout);
            let label = BranchLabel::new(&point, 0, false, &ctx);
            for width in 0..=3 {
                let actual = output(&label.render("", &String::new, Some(width)), Some(width));
                if width == 0 {
                    assert_eq!(actual, [""]);
                } else {
                    assert!(actual.last().unwrap().contains("+18446744073709551615 -0"));
                    assert_eq!(
                        actual.last().unwrap().contains("abcdef0"),
                        verbosity.includes_oid()
                    );
                    assert!(console::measure_text_width(&actual[0]) <= width);
                }
            }
        }
    }
    point.diff_stat = None;
    for verbosity in [Verbosity::Low, Verbosity::Medium, Verbosity::High] {
        let ctx = RenderContext::new(
            "main",
            None,
            None,
            None,
            TEST_NOW,
            verbosity,
            MetadataWidths::default(),
            &colours,
        );
        let rows = BranchLabel::new(&point, 0, false, &ctx).render("", &String::new, Some(8));
        assert_eq!(rows.len(), if verbosity.includes_oid() { 2 } else { 1 });
    }
    let point = super::point("abcdef0", &[]);
    let ctx = RenderContext::new(
        "main",
        None,
        Some("unknown"),
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    );
    assert_eq!(
        BranchLabel::new(&point, 0, false, &ctx).render("long prefix", &String::new, Some(2))[0],
        ".."
    );
}

#[test]
fn inline_sha_moves_to_the_continuation_and_orphan_status_uses_spare_space() {
    let colours = test_colours(false);
    let mut point = point_with_count("abcdef0", &["branch"], 1, "subject");
    point.diff_stat = Some(DiffStat {
        added: 0,
        deleted: 0,
    });
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    );
    let label = BranchLabel::new(&point, 0, true, &ctx);
    let rows = output(&label.render("", &String::new, Some(30)), Some(30));
    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("branch (orphaned)"));
    assert!(!rows[0].contains("abcdef0"));
    assert!(rows[1].ends_with("+0 -0 abcdef0"));
    assert_eq!(console::measure_text_width(&rows[1]), 30);
    let unknown = label.render("", &String::new, None);
    assert!(unknown[0].contains("(1, abcdef0) branch (orphaned) subject"));
}

#[test]
fn one_wide_layer_does_not_hide_other_counts_and_continuations_keep_lanes() {
    let mut short = point_with_count("shortid", &["a"], 1, "subject");
    short.diff_stat = Some(DiffStat {
        added: 1,
        deleted: 2,
    });
    let mut long = point_with_count("longoid", &["very-long-branch-name", "alias"], 1, "subject");
    long.diff_stat = Some(DiffStat {
        added: 3,
        deleted: 4,
    });
    let mut first = Lane::new(
        "shortid",
        Some("main".into()),
        vec![short, point("shared", &["shared"])],
        0,
        false,
    );
    first
        .rewritten_commits
        .push(crate::model::RewrittenCommit::new(
            super::meta("old", "old"),
            super::meta("new", "new"),
        ));
    let mut second = Lane::new(
        "longoid",
        Some("main".into()),
        vec![long, point("shared", &["shared"])],
        0,
        true,
    );
    second
        .rewritten_commits
        .clone_from(&first.rewritten_commits);
    for orphan in [false, true] {
        let groups = vec![LaneGroup::new(
            Some("main".into()),
            None,
            if orphan { None } else { Some(0) },
            vec![first.clone(), second.clone()],
        )];
        let topology = PreparedTopology::new(&groups);
        let colours = test_colours(false);
        let ctx = RenderContext::new(
            "main",
            None,
            Some("very-long-branch-name"),
            None,
            TEST_NOW,
            Verbosity::High,
            calculate_metadata_widths(&groups, None, TEST_NOW, Verbosity::High),
            &colours,
        )
        .with_layout(Layout::Columns)
        .with_diff_stats(&groups)
        .with_topology(&topology, Some(29));
        let lines = render_lane_groups(&groups, &ctx);
        let actual = output(&lines, Some(29));
        let short = actual
            .iter()
            .position(|line| line.ends_with("shortid"))
            .unwrap();
        let long = actual
            .iter()
            .position(|line| line.ends_with("longoid"))
            .unwrap();
        assert!(actual[short].contains("+1 -2"));
        assert!(actual[long].contains("+3 -4"));
        assert!(!actual[long].contains('▶'));
        assert!(!actual[long].contains('●'));
        assert!(actual[long].contains('│'));
        assert_eq!(actual.iter().filter(|line| line.contains('▶')).count(), 1);
        if !orphan {
            assert!(actual[short].contains('a'));
            assert!(actual.iter().any(|line| line.contains('├')));
            assert!(actual.iter().any(|line| line.contains('✕')));
        }
    }
}

#[test]
fn unannotated_history_rows_keep_their_label_without_layer_statistics() {
    let mut history = point("history", &["historical-reference"]);
    history.main_history = true;
    history.diff_stat = Some(DiffStat {
        added: 123,
        deleted: 456,
    });
    let groups = vec![LaneGroup::new(
        Some("main".into()),
        None,
        Some(0),
        vec![Lane::new(
            "history",
            Some("main".into()),
            vec![history],
            0,
            false,
        )],
    )];
    let colours = test_colours(false);
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    );
    let rows = render_lane_groups(&groups, &ctx);
    let history = rows
        .iter()
        .find(|row| row.contains("historical-reference"))
        .unwrap();
    assert!(history.ends_with("historical-reference"));
    assert!(!history.contains("+123"));
    assert!(!history.contains("-456"));
}

use super::*;
use crate::cli::{Layout, Verbosity};
use crate::model::{DiffStat, Lane};
use crate::render::{Colours, calculate_metadata_widths, render_lane_groups};
use crate::test_support::TEST_NOW;

fn point_with_count(oid: &str, names: &[&str], count: usize, subject: &str) -> BranchPoint {
    BranchPoint::new(
        oid,
        names.iter().copied(),
        Some(crate::model::BranchAnnotation::new(
            crate::model::CommitMeta::new(oid, 0, subject),
            count,
        )),
    )
}

fn groups() -> Vec<LaneGroup> {
    let mut point = point_with_count("abcdef0", &["topic"], 2, "long subject");
    point.diff_stat = Some(DiffStat {
        added: 128,
        deleted: 3,
    });
    let mut child = point_with_count("1234567", &["child"], 1, "child subject");
    child.diff_stat = Some(DiffStat {
        added: 2,
        deleted: 45,
    });
    vec![LaneGroup::new(
        Some("main".to_string()),
        None,
        Some(0),
        vec![Lane::new(
            "1234567",
            Some("main".to_string()),
            vec![child, point],
            0,
            false,
        )],
    )]
}

#[test]
fn aligns_each_count_and_uses_standard_ansi_colours() {
    let groups = groups();
    for enabled in [false, true] {
        let colours = Colours::new(enabled, crate::cli::Palette::Classic);
        let ctx = RenderContext::new(
            "main",
            None,
            None,
            None,
            TEST_NOW,
            Verbosity::High,
            calculate_metadata_widths(&groups, None, TEST_NOW, Verbosity::High),
            &colours,
        )
        .with_layout(Layout::Columns)
        .with_diff_stats(&groups);
        let lines = render_lane_groups(&groups, &ctx);
        let topic = lines.iter().find(|line| line.contains("topic")).unwrap();
        let child = lines.iter().find(|line| line.contains("child")).unwrap();
        assert!(console::strip_ansi_codes(topic).ends_with("+128  -3 abcdef0"));
        assert!(console::strip_ansi_codes(child).ends_with("  +2 -45 1234567"));
        if enabled {
            assert!(topic.contains("\x1b[32m+128\x1b[0m"));
            assert!(topic.contains("\x1b[31m -3\x1b[0m"));
        } else {
            assert!(!topic.contains('\x1b'));
        }
    }
}

#[test]
fn supports_low_verbosity_inline_and_orphaned_rows() {
    let mut groups = groups();
    let colours = Colours::new(false, crate::cli::Palette::Classic);
    for orphan in [false, true] {
        if orphan {
            groups[0].main_distance = None;
        }
        let ctx = RenderContext::new(
            "main",
            None,
            None,
            None,
            TEST_NOW,
            Verbosity::Low,
            crate::render::metadata::MetadataWidths::default(),
            &colours,
        )
        .with_diff_stats(&groups);
        let mut lines = render_lane_groups(&groups, &ctx);
        let original = lines.clone();
        let topic = lines.iter().find(|line| line.contains("topic")).unwrap();
        assert!(topic.ends_with("+128  -3"));
        assert!(!topic.contains("long subject"));
        fit_diff_stats(&mut lines, None);
        assert_eq!(lines, original);
        fit_diff_stats(&mut lines, Some(200));
        assert_eq!(lines, original);
        fit_diff_stats(&mut lines, Some(1));
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("+128") || line.contains("-45"))
        );
    }
}

#[test]
fn hides_all_stats_when_any_branch_would_be_truncated() {
    let mut lines = vec![
        RenderLine::with_fixed_suffix("branch with subject", "abcdef0")
            .with_diff_stat("+128 -3", 12),
        RenderLine::plain("short").with_diff_stat("+2 -45", 5),
        RenderLine::plain("main"),
    ];
    let original = lines.clone();
    fit_diff_stats(&mut lines, Some(28));
    assert_eq!(lines, original);
    fit_diff_stats(&mut lines, Some(27));
    assert_eq!(lines[0].as_str(), "branch with subject abcdef0");
    assert_eq!(
        lines[0].fixed_suffix(),
        Some(("branch with subject", "abcdef0"))
    );
    assert_eq!(lines[1].as_str(), "short");
}

#[test]
fn restores_empty_prefixes_and_plain_rows_without_retaining_duplicate_lines() {
    for original in [
        RenderLine::with_fixed_suffix("", "oid"),
        RenderLine::plain(""),
        RenderLine::plain("日本語"),
    ] {
        let mut line = original.clone().with_diff_stat("\x1b[32m+2\x1b[0m -0", 4);
        assert!(line.diff_stat_minimum_width().is_some());
        line.hide_diff_stat();
        assert_eq!(line, original);
        line.hide_diff_stat();
        assert_eq!(line, original);
    }
    assert_eq!(count_width(u64::MAX), 21);
}

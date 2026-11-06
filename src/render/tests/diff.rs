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
        let colours = Colours::new(enabled);
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
    let colours = Colours::new(false);
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
        let lines = render_lane_groups(&groups, &ctx);
        let topic = lines.iter().find(|line| line.contains("topic")).unwrap();
        assert!(topic.ends_with("+128  -3"));
        assert!(!topic.contains("long subject"));
        assert_eq!(
            format_diff_stat(&groups[0].lanes[0].branch_points[0], &ctx, false),
            "+2 -45"
        );
    }
    assert_eq!(count_width(u64::MAX), 21);
}

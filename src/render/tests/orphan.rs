use crate::cli::{Layout, Verbosity};
use crate::model::Lane;
use crate::render::RenderContext;
use crate::render::graph::COLLAPSED_MAIN_GLYPH;
use crate::render::metadata::MetadataWidths;
use crate::render::orphan::{display_orphaned_names, render_orphaned_group};
use crate::test_support::TEST_NOW;

use super::{point, point_with_count, test_colours};

#[test]
fn renders_orphaned_names_with_status_metadata_and_title() {
    let colours = test_colours(false);
    let point = point_with_count("backup-oid", &["backup"], 2, "backup tip");

    assert_eq!(
        display_orphaned_names(
            &point,
            TEST_NOW,
            Verbosity::High,
            MetadataWidths::default(),
            Layout::Inline,
            &colours,
        ),
        "2m (2, backup-) backup (orphaned) backup tip"
    );
}

#[test]
fn renders_orphaned_lane_with_single_warning_marker() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "backup".to_string(),
        base_oid: None,
        branch_points: vec![point("backup", &["test-branch-name"])],
        rewritten_commits: Vec::new(),
        head_timestamp: 1,
        contains_current: false,
    }];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("main"),
        Some("main"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_orphaned_group(&lanes, &ctx);

    assert_eq!(
        output,
        vec!["  ⁝ ⦸ test-branch-name (orphaned)".to_string()]
    );
}

#[test]
fn renders_orphaned_lane_with_main_coloured_history_marker() {
    let colours = test_colours(true);
    let lanes = vec![Lane::new(
        "backup",
        None,
        vec![point("backup", &["test-branch-name"])],
        1,
        false,
    )];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("main"),
        Some("main"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_orphaned_group(&lanes, &ctx);

    assert_eq!(
        output,
        vec![format!(
            "  {} {} {} {}",
            colours.stack(0, COLLAPSED_MAIN_GLYPH),
            colours.orphaned_glyph("⦸"),
            colours.orphaned_name("test-branch-name"),
            colours.orphaned_status("(orphaned)")
        )]
    );
}

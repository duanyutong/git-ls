use crate::cli::Verbosity;
use crate::model::{CommitMeta, Lane, LaneGroup};
use crate::render::RenderContext;
use crate::render::branch::display_names;
use crate::render::layout::render_lane_groups;
use crate::render::metadata::{MetadataWidths, calculate_metadata_widths, format_metadata_prefix};
use crate::render::trunk::{TrunkLabel, trunk_label};
use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

use super::{point_with_count, point_with_count_at, test_colours};

#[test]
fn formats_metadata_prefix_with_aligned_placeholders() {
    let colours = test_colours(false);
    let widths = MetadataWidths { age: 3, count: 2 };

    assert_eq!(
        format_metadata_prefix("2m", "--", "main-oi", widths, &colours),
        " 2m (--, main-oi)"
    );
    assert_eq!(
        crate::render::metadata::trunk_count_placeholder(widths),
        "--"
    );
}

#[test]
fn renders_trunk_commit_label_with_main_placeholder_count() {
    let colours = test_colours(false);
    let base_meta = CommitMeta::new("old-main", TEST_COMMIT_TIME, "old main point");
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::Medium,
        MetadataWidths { age: 2, count: 2 },
        &colours,
    );

    assert_eq!(
        trunk_label(TrunkLabel::Commit(&base_meta), &ctx),
        "2m (--, old-mai) old main point"
    );
}

#[test]
fn renders_branch_metadata_with_commit_count_for_multi_commit_branch() {
    let colours = test_colours(false);
    let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

    let label = display_names(
        &point,
        Some("other"),
        0,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    );

    assert_eq!(label, "2m (3, branch-) feature/topic finish topic");
}

#[test]
fn renders_summary_branch_metadata_without_commit_title() {
    let colours = test_colours(false);
    let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

    let label = display_names(
        &point,
        Some("other"),
        0,
        TEST_NOW,
        Verbosity::Medium,
        MetadataWidths::default(),
        &colours,
    );

    assert_eq!(label, "2m (3, branch-) feature/topic");
}

#[test]
fn renders_main_metadata_in_aligned_annotation_column() {
    let colours = test_colours(false);
    let main_meta = CommitMeta::new("main-oid", TEST_COMMIT_TIME, "main tip");
    let groups = vec![LaneGroup {
        base_oid: None,
        base_meta: None,
        main_distance: None,
        lanes: vec![Lane {
            head_oid: "backup-oid".to_string(),
            base_oid: None,
            branch_points: vec![point_with_count_at(
                "backup-oid",
                &["backup"],
                10,
                "backup tip",
                TEST_COMMIT_TIME,
            )],
            head_timestamp: TEST_COMMIT_TIME,
            contains_current: false,
        }],
    }];
    let metadata_widths =
        calculate_metadata_widths(&groups, Some(&main_meta), TEST_NOW, Verbosity::Medium);
    let ctx = RenderContext::new(
        "main",
        Some(&main_meta),
        Some("main"),
        Some("main-oid"),
        TEST_NOW,
        Verbosity::Medium,
        metadata_widths,
        &colours,
    );

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            "  ⁝".to_string(),
            "▶ ◆── 2m (--, main-oi) main".to_string(),
            "  ⁝ ⦸ 2m (10, backup-) backup (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

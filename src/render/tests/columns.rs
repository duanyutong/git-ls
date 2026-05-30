//! Behaviour of the `--layout columns` rendering: the age is relocated to
//! a fixed-width left gutter, the commit count is promoted to a highlighted
//! column following the rails, and full verbosity demotes the object identifier
//! to a trailing position. Colour is disabled throughout so the assertions read
//! as plain text.

use crate::cli::{Layout, Verbosity};
use crate::model::{CommitMeta, Lane, LaneGroup, RewrittenCommit};
use crate::render::RenderContext;
use crate::render::branch::display_names;
use crate::render::layout::render_lane_groups;
use crate::render::metadata::{
    MetadataWidths, age_gutter, calculate_metadata_widths, columns_count,
};
use crate::render::orphan::display_orphaned_names;
use crate::render::rewrite::display_rewritten_commit;
use crate::render::trunk::{TrunkLabel, trunk_age, trunk_label};
use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

use super::{meta, point_with_count, point_with_count_at, test_colours};

fn columns_context<'a>(
    main_meta: Option<&'a CommitMeta>,
    widths: MetadataWidths,
    colours: &'a crate::render::Colours,
) -> RenderContext<'a> {
    RenderContext::new(
        "main",
        main_meta,
        None,
        None,
        TEST_NOW,
        Verbosity::Medium,
        widths,
        colours,
    )
    .with_layout(Layout::Columns)
}

#[test]
fn age_gutter_reserves_a_fixed_width_column_for_every_row() {
    let colours = test_colours(false);
    let ctx = columns_context(None, MetadataWidths { age: 3, count: 2 }, &colours);

    assert_eq!(age_gutter(&ctx, Some("2m".to_string())), " 2m ");
    assert_eq!(age_gutter(&ctx, None), "    ");
}

#[test]
fn age_gutter_is_empty_without_columns_or_metadata() {
    let colours = test_colours(false);

    let inline = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::Medium,
        MetadataWidths { age: 3, count: 2 },
        &colours,
    )
    .with_layout(Layout::Inline);
    assert_eq!(age_gutter(&inline, Some("2m".to_string())), "");

    let columns_without_metadata = columns_context(None, MetadataWidths::default(), &colours);
    assert_eq!(age_gutter(&columns_without_metadata, None), "");
}

#[test]
fn columns_count_highlights_real_counts_and_mutes_placeholders() {
    let colours = test_colours(false);
    let widths = MetadataWidths { age: 2, count: 2 };

    assert_eq!(columns_count("3", widths, &colours, false), " 3");
    assert_eq!(columns_count("--", widths, &colours, true), "--");
}

#[test]
fn display_names_promotes_count_and_trails_the_identifier() {
    let colours = test_colours(false);
    let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

    assert_eq!(
        display_names(
            &point,
            Some("other"),
            0,
            TEST_NOW,
            Verbosity::High,
            MetadataWidths { age: 2, count: 2 },
            Layout::Columns,
            &colours,
        ),
        " 3 feature/topic finish topic branch-"
    );
    assert_eq!(
        display_names(
            &point,
            Some("other"),
            0,
            TEST_NOW,
            Verbosity::Medium,
            MetadataWidths { age: 2, count: 2 },
            Layout::Columns,
            &colours,
        ),
        " 3 feature/topic"
    );
}

#[test]
fn trunk_labels_render_muted_placeholder_count_and_trailing_identifier() {
    let colours = test_colours(false);
    let main_meta = CommitMeta::new("main-oid", TEST_COMMIT_TIME, "main tip");
    let widths = MetadataWidths { age: 2, count: 2 };
    let ctx = columns_context(Some(&main_meta), widths, &colours);

    assert_eq!(trunk_label(TrunkLabel::Main, &ctx), "-- main");

    let base_meta = CommitMeta::new("old-main", TEST_COMMIT_TIME, "old main point");
    assert_eq!(trunk_label(TrunkLabel::Commit(&base_meta), &ctx), "--");

    let detail_ctx = RenderContext::new(
        "main",
        Some(&main_meta),
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        widths,
        &colours,
    )
    .with_layout(Layout::Columns);
    assert_eq!(
        trunk_label(TrunkLabel::Main, &detail_ctx),
        "-- main main-oi"
    );
    assert_eq!(
        trunk_label(TrunkLabel::Commit(&base_meta), &detail_ctx),
        "-- old main point old-mai"
    );
}

#[test]
fn trunk_age_resolves_the_gutter_value_per_label() {
    let colours = test_colours(false);
    let main_meta = CommitMeta::new("main-oid", TEST_COMMIT_TIME, "main tip");
    let base_meta = CommitMeta::new("old-main", TEST_COMMIT_TIME, "old main point");
    let widths = MetadataWidths { age: 2, count: 2 };

    let ctx = columns_context(Some(&main_meta), widths, &colours);
    assert_eq!(trunk_age(TrunkLabel::Main, &ctx), Some("2m".to_string()));
    assert_eq!(
        trunk_age(TrunkLabel::Commit(&base_meta), &ctx),
        Some("2m".to_string())
    );

    let without_main = columns_context(None, widths, &colours);
    assert_eq!(trunk_age(TrunkLabel::Main, &without_main), None);

    let low = RenderContext::new(
        "main",
        Some(&main_meta),
        None,
        None,
        TEST_NOW,
        Verbosity::Low,
        widths,
        &colours,
    )
    .with_layout(Layout::Columns);
    assert_eq!(trunk_age(TrunkLabel::Main, &low), None);
}

#[test]
fn rewritten_commit_drops_parentheses_and_keeps_identifiers_inline() {
    let colours = test_colours(false);
    let widths = MetadataWidths { age: 2, count: 2 };
    let commit = RewrittenCommit::new(meta("old-oid", "reworked topic"), meta("new-oid", "new"));

    let ctx_summary = columns_context(None, widths, &colours);
    assert_eq!(
        display_rewritten_commit(&commit, &ctx_summary),
        "   rewritten"
    );

    let ctx_title = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        widths,
        &colours,
    )
    .with_layout(Layout::Columns);
    assert_eq!(
        display_rewritten_commit(&commit, &ctx_title),
        "   old-oid rewritten as new-oid reworked topic"
    );
}

#[test]
fn orphaned_names_promote_count_and_trail_the_identifier() {
    let colours = test_colours(false);
    let point = point_with_count("backup-oid", &["backup"], 2, "backup tip");
    let widths = MetadataWidths { age: 2, count: 2 };

    assert_eq!(
        display_orphaned_names(
            &point,
            TEST_NOW,
            Verbosity::High,
            widths,
            Layout::Columns,
            &colours,
        ),
        " 2 backup (orphaned) backup tip backup-"
    );
    assert_eq!(
        display_orphaned_names(
            &point,
            TEST_NOW,
            Verbosity::Medium,
            widths,
            Layout::Columns,
            &colours,
        ),
        " 2 backup (orphaned)"
    );
}

#[test]
fn render_lane_groups_aligns_the_age_gutter_across_every_row() {
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
            rewritten_commits: Vec::new(),
            head_timestamp: TEST_COMMIT_TIME,
            contains_current: false,
        }],
    }];
    let widths = calculate_metadata_widths(&groups, Some(&main_meta), TEST_NOW, Verbosity::Medium);
    let ctx = RenderContext::new(
        "main",
        Some(&main_meta),
        Some("main"),
        Some("main-oid"),
        TEST_NOW,
        Verbosity::Medium,
        widths,
        &colours,
    )
    .with_layout(Layout::Columns);

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            "     ⁝".to_string(),
            "2m ▶ ◆── -- main".to_string(),
            "2m   ⁝ ⦸ 10 backup (orphaned)".to_string(),
            "     ⁝".to_string(),
        ]
    );
}

use crate::cli::Verbosity;
use crate::model::CommitMeta;
use crate::render::RenderContext;
use crate::render::graph::{LaneRenderLayout, MainSpine};
use crate::render::metadata::MetadataWidths;
use crate::render::trunk::{TrunkLabel, render_collapsed_main_segment, trunk_label, trunk_prefix};
use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

use super::test_colours;

#[test]
fn trunk_prefix_omits_connectors_when_main_spine_is_hidden() {
    let colours = test_colours(false);
    let layout = LaneRenderLayout::new(1, 1, 1);

    assert_eq!(
        trunk_prefix(layout, false, MainSpine::Hidden, &colours),
        "◇"
    );
}

#[test]
fn collapsed_main_segment_uses_singular_commit_label() {
    let colours = test_colours(false);
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature"),
        Some("feature"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_collapsed_main_segment(1, &ctx)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(
        output,
        vec![
            "  │".to_string(),
            "  ⁝ (1 commit on main)".to_string(),
            "  │".to_string(),
        ]
    );
}

#[test]
fn inline_trunk_commit_label_includes_metadata_title() {
    let colours = test_colours(false);
    let base_meta = CommitMeta::new("old-main", TEST_COMMIT_TIME, "old main point");
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths { age: 2, count: 2 },
        &colours,
    );

    assert_eq!(
        trunk_label(TrunkLabel::Commit(&base_meta), &ctx),
        "2m (--, old-mai) old main point"
    );
}

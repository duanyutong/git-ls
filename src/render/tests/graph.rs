use crate::render::graph::{
    LaneRenderLayout, MainSpine, current_row_indicator, marker_for, orphaned_row_indicator,
    row_prefix,
};

use super::{point, test_colours};

#[test]
fn selects_branch_markers_by_current_branch_then_head() {
    let current = point("current-oid", &["feature/current"]);
    let head = point("head-oid", &["feature/head"]);
    let other = point("other-oid", &["feature/other"]);

    assert_eq!(marker_for(&current, Some("feature/current"), None), "●");
    assert_eq!(marker_for(&head, Some("main"), Some("head-oid")), "◉");
    assert_eq!(marker_for(&other, Some("main"), Some("head-oid")), "◯");
}

#[test]
fn renders_current_and_orphaned_row_indicators() {
    let colours = test_colours(false);

    assert_eq!(current_row_indicator(true, 0, &colours), "▶");
    assert_eq!(current_row_indicator(false, 0, &colours), " ");
    assert_eq!(orphaned_row_indicator(true, &colours), "▶");
    assert_eq!(orphaned_row_indicator(false, &colours), " ");
}

#[test]
fn row_prefix_can_hide_main_spine_for_branch_only_rows() {
    let colours = test_colours(false);
    let point = point("feature", &["feature"]);
    let layout = LaneRenderLayout::new(1, 1, 1);

    assert_eq!(
        row_prefix(
            0,
            layout,
            &point,
            Some("main"),
            Some("main"),
            MainSpine::Hidden,
            &colours,
        ),
        "◯"
    );
}

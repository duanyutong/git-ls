use crate::render::graph::{current_row_indicator, marker_for, orphaned_row_indicator};

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

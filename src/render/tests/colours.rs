use crate::cli::Verbosity;
use crate::model::{Lane, LaneGroup};
use crate::render::RenderContext;
use crate::render::graph::{LaneRenderLayout, MainSpine};
use crate::render::layout::render_lane_groups;
use crate::render::metadata::MetadataWidths;
use crate::render::trunk::{main_label, trunk_prefix};
use crate::test_support::TEST_NOW;

use super::{point, test_colours};

#[test]
fn colours_text_when_enabled() {
    let colours = test_colours(true);

    assert_eq!(colours.stack(0, "x"), "\x1b[38;5;41mx\x1b[0m");
    assert_eq!(
        colours.current_stack(0, "x"),
        "\x1b[1m\x1b[4m\x1b[38;5;41mx\x1b[0m"
    );
    assert_eq!(
        colours.current_indicator(0, "x"),
        "\x1b[1m\x1b[38;5;41mx\x1b[0m"
    );
    assert_eq!(colours.dim("x"), "\x1b[2mx\x1b[0m");
    assert_eq!(colours.muted_text("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_age("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_count("x"), "\x1b[38;5;255mx\x1b[0m");
    assert_eq!(colours.metadata_oid("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_punctuation("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.commit_title("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.orphaned_name("x"), "\x1b[38;5;255mx\x1b[0m");
    assert_eq!(colours.orphaned_glyph("x"), "\x1b[1m\x1b[38;5;255mx\x1b[0m");
    assert_eq!(
        colours.orphaned_status("x"),
        "\x1b[1m\x1b[38;5;255mx\x1b[0m"
    );
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
    assert_eq!(main_label(&ctx), "\x1b[1m\x1b[4m\x1b[38;5;41mmain\x1b[0m");
    assert_eq!(
        trunk_prefix(LaneRenderLayout::empty(), true, MainSpine::Hidden, &colours),
        "\x1b[38;5;41m◆\x1b[0m\x1b[38;5;41m──\x1b[0m"
    );
    let inactive_main_ctx = RenderContext::new(
        "main",
        None,
        Some("feature"),
        Some("feature"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );
    assert_eq!(main_label(&inactive_main_ctx), "\x1b[38;5;41mmain\x1b[0m");
    assert_eq!(
        trunk_prefix(
            LaneRenderLayout::empty(),
            false,
            MainSpine::Hidden,
            &colours
        ),
        "\x1b[38;5;41m◇\x1b[0m\x1b[38;5;41m──\x1b[0m"
    );
}

#[test]
fn main_reserves_first_palette_colour() {
    let colours = test_colours(true);
    let groups = vec![LaneGroup {
        base_oid: Some("main".to_string()),
        base_meta: None,
        main_distance: Some(0),
        lanes: vec![Lane {
            head_oid: "feature".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("feature", &["feature/one"])],
            rewritten_commits: Vec::new(),
            head_timestamp: 1,
            contains_current: false,
        }],
    }];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature/one"),
        Some("feature"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_lane_groups(&groups, &ctx);

    assert!(output[1].contains("\x1b[38;5;203m●\x1b[0m"));
    assert!(output[2].contains("\x1b[38;5;41m◇\x1b[0m"));
    assert!(output[2].contains("\x1b[38;5;41mmain\x1b[0m"));
}

use crate::cli::Verbosity;
use crate::model::{Lane, LaneGroup};
use crate::render::RenderContext;
use crate::render::graph::{COLLAPSED_MAIN_GLYPH, MainSpine};
use crate::render::layout::{render_group, render_lane_groups};
use crate::render::metadata::MetadataWidths;
use crate::render::trunk::TrunkLabel;
use crate::test_support::TEST_NOW;

use super::{meta, point, test_colours};

#[test]
fn renders_markers_names_and_trunk() {
    let colours = test_colours(false);
    let lanes = vec![
        Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        },
        Lane {
            head_oid: "b".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("b", &["feature/two"])],
            head_timestamp: 2,
            contains_current: true,
        },
    ];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature/two"),
        Some("b"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec![
            "    ◯   feature/one".to_string(),
            "▶ ⁝ │ ● feature/two".to_string(),
            "  ◇─┴─┘ main".to_string()
        ]
    );
}

#[test]
fn renders_exactly_one_future_line_above_main_node() {
    let colours = test_colours(false);
    let lanes = vec![
        Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        },
        Lane {
            head_oid: "b".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("b", &["feature/two"])],
            head_timestamp: 2,
            contains_current: true,
        },
    ];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature/two"),
        Some("b"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output
            .iter()
            .filter(|line| line.contains(COLLAPSED_MAIN_GLYPH))
            .count(),
        1
    );
    assert_eq!(output[output.len() - 2], "▶ ⁝ │ ● feature/two");
    assert_eq!(output[output.len() - 1], "  ◇─┴─┘ main");
}

#[test]
fn renders_single_main_based_lane_with_main_spine() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "a".to_string(),
        base_oid: Some("main".to_string()),
        branch_points: vec![point("a", &["feature/one"])],
        head_timestamp: 1,
        contains_current: true,
    }];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature/one"),
        Some("a"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec!["▶ ⁝ ● feature/one".to_string(), "  ◇─┘ main".to_string()]
    );
}

#[test]
fn renders_current_main_on_trunk_row() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "a".to_string(),
        base_oid: Some("main".to_string()),
        branch_points: vec![point("a", &["feature/one"])],
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

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec!["  ⁝ ◯ feature/one".to_string(), "▶ ◆─┘ main".to_string()]
    );
}

#[test]
fn renders_orphaned_only_groups_around_main_tip() {
    let colours = test_colours(false);
    let groups = vec![LaneGroup {
        base_oid: None,
        base_meta: None,
        main_distance: None,
        lanes: vec![Lane {
            head_oid: "backup".to_string(),
            base_oid: None,
            branch_points: vec![point("backup", &["test-branch-name"])],
            head_timestamp: 1,
            contains_current: false,
        }],
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

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            "  ⁝".to_string(),
            "▶ ◆── main".to_string(),
            "  ⁝ ⦸ test-branch-name (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_orphaned_groups_below_connected_stacks() {
    let colours = test_colours(false);
    let groups = vec![
        LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: None,
            main_distance: Some(0),
            lanes: vec![Lane {
                head_oid: "feature".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("feature", &["feature/current"])],
                head_timestamp: 2,
                contains_current: true,
            }],
        },
        LaneGroup {
            base_oid: None,
            base_meta: None,
            main_distance: None,
            lanes: vec![
                Lane {
                    head_oid: "orphan-a".to_string(),
                    base_oid: None,
                    branch_points: vec![point("orphan-a", &["orphan-A"])],
                    head_timestamp: 1,
                    contains_current: false,
                },
                Lane {
                    head_oid: "orphan-b".to_string(),
                    base_oid: None,
                    branch_points: vec![point("orphan-b", &["orphan-B"])],
                    head_timestamp: 1,
                    contains_current: false,
                },
            ],
        },
    ];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("feature/current"),
        Some("feature"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            String::new(),
            "▶ ⁝ ● feature/current".to_string(),
            "  ◇─┘ main".to_string(),
            "  ⁝ ⦸ orphan-A (orphaned)".to_string(),
            "  ⁝ ⦸ orphan-B (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_old_main_groups_with_collapsed_main_history() {
    let colours = test_colours(false);
    let groups = vec![
        LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: Some(meta("main", "main tip")),
            main_distance: Some(0),
            lanes: vec![
                Lane {
                    head_oid: "feature-one".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-one", &["feature/one"])],
                    head_timestamp: 4,
                    contains_current: false,
                },
                Lane {
                    head_oid: "feature-two".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-two", &["feature/two"])],
                    head_timestamp: 3,
                    contains_current: false,
                },
                Lane {
                    head_oid: "feature-current".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-current", &["feature/current"])],
                    head_timestamp: 2,
                    contains_current: false,
                },
            ],
        },
        LaneGroup {
            base_oid: Some("old-main".to_string()),
            base_meta: Some(meta(
                "old-main",
                "chore: this is an old commit in main history",
            )),
            main_distance: Some(842),
            lanes: vec![Lane {
                head_oid: "old-feature".to_string(),
                base_oid: Some("old-main".to_string()),
                branch_points: vec![point("old-feature", &["dyt/tgs_api"])],
                head_timestamp: 1,
                contains_current: true,
            }],
        },
    ];
    let ctx = RenderContext::new(
        "main",
        None,
        Some("dyt/tgs_api"),
        Some("old-feature"),
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            String::new(),
            "    ◯     feature/one".to_string(),
            "    │ ◯   feature/two".to_string(),
            "  ⁝ │ │ ◯ feature/current".to_string(),
            "  ◇─┴─┴─┘ main".to_string(),
            "  │".to_string(),
            "  ⁝ (842 commits on main)".to_string(),
            "  │".to_string(),
            "▶ │     ● dyt/tgs_api".to_string(),
            "  ◇─────┘ chore: this is an old commit in main history".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_main_tip_before_first_group_from_older_main_history() {
    let colours = test_colours(false);
    let groups = vec![LaneGroup {
        base_oid: Some("old-main".to_string()),
        base_meta: Some(meta("old-main", "old main point")),
        main_distance: Some(2),
        lanes: vec![Lane {
            head_oid: "feature".to_string(),
            base_oid: Some("old-main".to_string()),
            branch_points: vec![point("feature", &["feature"])],
            head_timestamp: 1,
            contains_current: false,
        }],
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

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(output[1], "▶ ◆── main");
    assert!(output.iter().any(|line| line == "  ⁝ (2 commits on main)"));
}

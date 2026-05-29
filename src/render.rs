mod branch;
mod colours;
mod context;
mod graph;
mod layout;
mod metadata;
mod orphan;
mod trunk;

pub(crate) use colours::Colours;
pub(crate) use context::RenderContext;
pub(crate) use layout::render_lane_groups;
pub(crate) use metadata::{calculate_metadata_widths, current_unix_timestamp};
pub(crate) use trunk::{render_main_tip, render_omitted_main_past, render_top_spacer};

#[cfg(test)]
use branch::display_names;
#[cfg(test)]
use graph::{
    COLLAPSED_MAIN_GLYPH, LaneRenderLayout, MainSpine, current_row_indicator, marker_for,
    orphaned_row_indicator,
};
#[cfg(test)]
use layout::render_group;
#[cfg(test)]
use metadata::{MetadataWidths, format_metadata_prefix, trunk_count_placeholder};
#[cfg(test)]
use orphan::{display_orphaned_names, render_orphaned_group};
#[cfg(test)]
use trunk::{TrunkLabel, main_label, trunk_label, trunk_prefix};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Palette, Verbosity};
    use crate::model::{BranchAnnotation, BranchPoint, CommitMeta, Lane, LaneGroup};
    use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

    fn point(oid: &str, names: &[&str]) -> BranchPoint {
        BranchPoint {
            oid: oid.to_string(),
            names: names.iter().map(|name| (*name).to_string()).collect(),
            annotation: None,
        }
    }

    fn point_with_count(
        oid: &str,
        names: &[&str],
        commit_count: usize,
        subject: &str,
    ) -> BranchPoint {
        point_with_count_at(oid, names, commit_count, subject, TEST_COMMIT_TIME)
    }

    fn point_with_count_at(
        oid: &str,
        names: &[&str],
        commit_count: usize,
        subject: &str,
        timestamp: i64,
    ) -> BranchPoint {
        BranchPoint {
            oid: oid.to_string(),
            names: names.iter().map(|name| (*name).to_string()).collect(),
            annotation: Some(BranchAnnotation {
                meta: CommitMeta {
                    oid: oid.to_string(),
                    short_oid: oid.to_string(),
                    subject: subject.to_string(),
                    timestamp,
                },
                commit_count,
            }),
        }
    }

    fn meta(oid: &str, subject: &str) -> CommitMeta {
        CommitMeta {
            oid: oid.to_string(),
            short_oid: oid.to_string(),
            subject: subject.to_string(),
            timestamp: 0,
        }
    }

    fn test_colours(enabled: bool) -> Colours {
        Colours {
            enabled,
            palette: Palette::Classic.ansi_colours(),
        }
    }

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
    fn formats_metadata_prefix_with_aligned_placeholders() {
        let colours = test_colours(false);
        let widths = MetadataWidths { age: 3, count: 2 };

        assert_eq!(
            format_metadata_prefix("2m", "--", "main-oi", widths, &colours),
            " 2m (--, main-oi)"
        );
        assert_eq!(trunk_count_placeholder(widths), "--");
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
    fn renders_trunk_commit_label_with_main_placeholder_count() {
        let colours = test_colours(false);
        let base_meta = CommitMeta {
            oid: "old-main".to_string(),
            short_oid: "old-mai".to_string(),
            subject: "old main point".to_string(),
            timestamp: TEST_COMMIT_TIME,
        };
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: None,
            head: None,
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Medium,
            metadata_widths: MetadataWidths { age: 2, count: 2 },
            colours: &colours,
        };

        assert_eq!(
            trunk_label(TrunkLabel::Commit(&base_meta), &ctx),
            "2m (--, old-mai) old main point"
        );
    }

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
                &colours,
            ),
            "2m (2, backup-oid) backup (orphaned) backup tip"
        );
    }

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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/two"),
            head: Some("b"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/two"),
            head: Some("b"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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

        assert_eq!(label, "2m (3, branch-head) feature/topic finish topic");
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

        assert_eq!(label, "2m (3, branch-head) feature/topic");
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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/one"),
            head: Some("a"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
    fn renders_orphaned_lane_with_single_warning_marker() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "backup".to_string(),
            base_oid: None,
            branch_points: vec![point("backup", &["test-branch-name"])],
            head_timestamp: 1,
            contains_current: false,
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_orphaned_group(&lanes, &ctx);

        assert_eq!(
            output,
            vec!["  ⁝ ⦸ test-branch-name (orphaned)".to_string()]
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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
    fn renders_main_metadata_in_aligned_annotation_column() {
        let colours = test_colours(false);
        let main_meta = CommitMeta {
            oid: "main-oid".to_string(),
            short_oid: "main-oi".to_string(),
            subject: "main tip".to_string(),
            timestamp: TEST_COMMIT_TIME,
        };
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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: Some(&main_meta),
            current_branch: Some("main"),
            head: Some("main-oid"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Medium,
            metadata_widths,
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                "  ⁝".to_string(),
                "▶ ◆── 2m (--, main-oi) main".to_string(),
                "  ⁝ ⦸ 2m (10, backup-oid) backup (orphaned)".to_string(),
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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/current"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("dyt/tgs_api"),
            head: Some("old-feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

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
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };
        assert_eq!(main_label(&ctx), "\x1b[1m\x1b[4m\x1b[38;5;41mmain\x1b[0m");
        assert_eq!(
            trunk_prefix(LaneRenderLayout::empty(), true, MainSpine::Hidden, &colours),
            "\x1b[38;5;41m◆\x1b[0m\x1b[38;5;41m──\x1b[0m"
        );
        let inactive_main_ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };
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
                head_timestamp: 1,
                contains_current: false,
            }],
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/one"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert!(output[1].contains("\x1b[38;5;203m●\x1b[0m"));
        assert!(output[2].contains("\x1b[38;5;41m◇\x1b[0m"));
        assert!(output[2].contains("\x1b[38;5;41mmain\x1b[0m"));
    }
}

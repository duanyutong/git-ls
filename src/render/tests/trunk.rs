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

#[test]
fn cached_mainline_labels_and_gap_names_remain_explicit_in_every_layout_and_verbosity() {
    use crate::cli::Layout;
    use crate::model::{LaneGroup, MainReference, RepositorySnapshot};
    use crate::render::layout::render_lane_groups;
    use crate::render::trunk::{reference_label, trunk_is_current};
    let colours = test_colours(false);
    let remote = CommitMeta::new("remote", TEST_COMMIT_TIME, "remote main commit");
    let local = CommitMeta::new("local", TEST_COMMIT_TIME, "local main commit");
    for layout in [Layout::Inline, Layout::Columns] {
        for verbosity in [Verbosity::Low, Verbosity::Medium, Verbosity::High] {
            for branch in [None, Some("main"), Some("feature"), Some("origin/main")] {
                let mut repository =
                    RepositorySnapshot::new(branch.map(str::to_string), None, "main");
                repository.main_oid = Some("remote".into());
                repository.main_refs = vec![
                    MainReference {
                        name: "main".into(),
                        oid: "local".into(),
                        cached: false,
                    },
                    MainReference {
                        name: "origin/main".into(),
                        oid: "remote".into(),
                        cached: true,
                    },
                ];
                let ctx = RenderContext::new(
                    "main",
                    Some(&remote),
                    branch,
                    None,
                    TEST_NOW,
                    verbosity,
                    MetadataWidths::default(),
                    &colours,
                )
                .with_layout(layout)
                .with_mainline(&repository);
                assert!(!trunk_is_current(TrunkLabel::Main, &ctx));
                assert_eq!(
                    trunk_is_current(TrunkLabel::Commit(&local), &ctx),
                    branch == Some("main")
                );
                assert!(
                    reference_label("remote", Some(&remote), &ctx)
                        .unwrap()
                        .contains("origin/main (cached)")
                );
                assert!(reference_label("missing", None, &ctx).is_none());
                let groups = vec![LaneGroup::new(
                    Some("local".into()),
                    Some(local.clone()),
                    Some(2),
                    Vec::new(),
                )];
                let lines = render_lane_groups(&groups, &ctx);
                let text = lines
                    .iter()
                    .map(crate::render::RenderLine::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                let remote_index = text.find("origin/main (cached)").unwrap();
                let gap_index = text.find("(2 commits on origin/main)").unwrap();
                let local_index = text.rfind("main").unwrap();
                assert!(
                    remote_index < gap_index && gap_index < local_index,
                    "{text}"
                );
                assert_eq!(text.contains('◆'), branch == Some("main"), "{text}");
            }
        }
    }
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::Low,
        MetadataWidths::default(),
        &colours,
    );
    let ctx = RenderContext {
        main_refs: &[crate::model::MainReference {
            name: "origin/main".into(),
            oid: "remote".into(),
            cached: true,
        }],
        ..ctx
    };
    assert!(!trunk_is_current(TrunkLabel::Main, &ctx));
}

#[test]
fn divergent_main_history_uses_diamonds_and_never_renders_feature_counts_or_statistics() {
    use super::{meta, point_with_count};
    use crate::cli::Layout;
    use crate::model::{DiffStat, Lane, LaneGroup, MainReference, RepositorySnapshot};
    use crate::render::layout::render_lane_groups;
    let colours = test_colours(false);
    let remote = meta("remote", "remote main");
    let local_meta = meta("local", "local main");
    for layout in [Layout::Inline, Layout::Columns] {
        for verbosity in [Verbosity::Low, Verbosity::High] {
            for branch in [None, Some("main")] {
                let mut repository =
                    RepositorySnapshot::new(branch.map(str::to_string), None, "main");
                repository.main_oid = Some("remote".into());
                repository.main_refs = vec![
                    MainReference {
                        name: "main".into(),
                        oid: "local".into(),
                        cached: false,
                    },
                    MainReference {
                        name: "origin/main".into(),
                        oid: "remote".into(),
                        cached: true,
                    },
                ];
                let ctx = RenderContext::new(
                    "main",
                    Some(&remote),
                    branch,
                    None,
                    TEST_NOW,
                    verbosity,
                    MetadataWidths::default(),
                    &colours,
                )
                .with_layout(layout)
                .with_mainline(&repository);
                let mut local = point_with_count("local", &["main"], 2, &local_meta.subject);
                local.main_history = true;
                local.diff_stat = Some(DiffStat {
                    added: 99,
                    deleted: 99,
                });
                let mut fork = point_with_count("fork", &[], 2, "local fork commit");
                fork.main_history = true;
                let groups = vec![LaneGroup::new(
                    Some("root".into()),
                    Some(meta("root", "common main")),
                    Some(1),
                    vec![Lane::new(
                        "local",
                        Some("root".into()),
                        vec![local, fork],
                        TEST_COMMIT_TIME,
                        branch.is_some(),
                    )],
                )];
                let lines = render_lane_groups(&groups, &ctx);
                let text = lines
                    .iter()
                    .map(crate::render::RenderLine::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(text.contains("local fork commit"), "{text}");
                assert_eq!(text.contains('◆'), branch.is_some(), "{text}");
                assert!(!text.contains("+99") && !text.contains("(2,"), "{text}");
            }
        }
    }
}

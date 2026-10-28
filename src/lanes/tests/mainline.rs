use super::*;
use crate::model::MainReference;

fn upstream(oid: &str) -> MainReference {
    MainReference {
        name: "origin/main".into(),
        oid: oid.into(),
        cached: true,
    }
}

fn tracked() -> FakeLaneBackend {
    FakeLaneBackend {
        upstreams: HashMap::from([("main".into(), upstream("remote"))]),
        ..FakeLaneBackend::default()
    }
}

#[test]
fn mainline_resolution_uses_ancestry_and_preserves_reference_identity() {
    for (local, base, expected) in [
        ("local", Some("local"), "remote"),
        ("local", Some("remote"), "local"),
        ("local", Some("root"), "remote"),
        ("local", None, "remote"),
        ("remote", None, "remote"),
    ] {
        let git = tracked()
            .with_head(Some(local), Some("main"))
            .with_merge_base(local, "remote", base);
        let (main, repository) = mainline_repository(&git, "main", local).unwrap();
        assert_eq!(main, expected);
        assert_eq!(repository.main_oid.as_deref(), Some(expected));
        assert_eq!(
            repository.main_refs,
            vec![
                MainReference {
                    name: "main".into(),
                    oid: local.into(),
                    cached: false
                },
                upstream("remote")
            ]
        );
        assert_eq!(repository.current_branch.as_deref(), Some("main"));
    }
    assert!(
        mainline_repository(
            &FakeLaneBackend {
                upstream_error: true,
                ..FakeLaneBackend::default()
            },
            "main",
            "local"
        )
        .is_err()
    );
    assert!(mainline_repository(&tracked(), "main", "local").is_err());
}

#[test]
fn selection_recognises_mainline_fork_points_and_removes_published_branches() {
    let git = tracked()
        .with_merge_base("local", "remote", Some("root"))
        .with_merge_base("local", "local", Some("local"))
        .with_merge_base("remote", "remote", Some("remote"))
        .with_merge_base("feature", "remote", Some("remote"))
        .with_merge_base("published", "remote", Some("published"))
        .with_merge_base("local", "feature", Some("fork"))
        .with_merge_base("fork", "remote", Some("root"));
    let (main, repository) = mainline_repository(&git, "main", "local").unwrap();
    let mut points = HashMap::from([
        (
            "feature".into(),
            BranchPointRef::new("feature", ["feature/topic"]),
        ),
        (
            "published".into(),
            BranchPointRef::new("published", ["feature/published"]),
        ),
    ]);
    include_mainline_points(&git, &main, &repository, &mut points).unwrap();
    assert!(!points.contains_key("published"));
    assert!(!points["feature"].main_history);
    assert!(points["fork"].main_history && points["fork"].names.is_empty());
    assert!(points["local"].main_history);
    assert_eq!(points["local"].names, vec!["main"]);
    let mut empty = HashMap::new();
    let (_, plain) = mainline_repository(&FakeLaneBackend::default(), "main", "local").unwrap();
    include_mainline_points(&git, &main, &plain, &mut empty).unwrap();
    let broken = FakeLaneBackend::default();
    assert!(include_mainline_points(&broken, &main, &repository, &mut points).is_err());
}

#[test]
fn branchless_selection_reconciles_its_selected_branches_with_the_cached_mainline() {
    let git = tracked()
        .with_local_branches(&[("local", &["main"]), ("feature", &["feature/topic"])])
        .with_revset("main()", false, &["local"])
        .with_branch_names(
            "((all()) & branches()) - public()",
            false,
            &["feature/topic"],
        )
        .with_revset(
            "heads(((all()) & branches()) - public())",
            false,
            &["feature"],
        )
        .with_merge_base("local", "remote", Some("local"))
        .with_merge_base("remote", "remote", Some("remote"))
        .with_merge_base("feature", "remote", Some("remote"));
    let selection = query_branchless_lane_selection(&git, "all()", false, &())
        .unwrap()
        .unwrap();
    let LaneSelection::Populated {
        main_oid,
        points_by_oid,
        head_oids,
        ..
    } = selection
    else {
        panic!("expected feature selection")
    };
    assert_eq!(main_oid, "remote");
    assert_eq!(head_oids, vec!["feature"]);
    assert_eq!(points_by_oid.len(), 1);
    let empty = git.with_branch_names("((all()) & branches()) - public()", false, &[]);
    assert!(
        matches!(query_branchless_lane_selection(&empty, "all()", false, &()).unwrap(), Some(LaneSelection::Empty { main_oid, .. }) if main_oid == "remote")
    );
}

#[test]
fn reference_endpoints_are_inserted_once_and_ordered_with_existing_main_groups() {
    let git = tracked()
        .with_merge_base("local", "remote", Some("local"))
        .with_ancestry_path(Some("local"), "remote", &["middle", "remote"])
        .with_meta("local", "local main", 1);
    let (main, repository) = mainline_repository(&git, "main", "local").unwrap();
    let mut groups = Vec::new();
    let mut metadata = HashMap::new();
    for _ in 0..2 {
        include_mainline_groups(
            &git,
            &mut groups,
            &main,
            &repository,
            Order::Newest,
            &mut metadata,
        )
        .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].main_distance, Some(2));
        assert_eq!(groups[0].lanes, Vec::<Lane>::new());
    }
    let mut anonymous = BranchPointRef::new("local", std::iter::empty::<String>());
    anonymous.main_history = true;
    let point = build_branch_point(
        &git,
        &BranchPointOnPath {
            point: anonymous,
            commit_count: 2,
        },
        Verbosity::Low,
        &mut metadata,
    )
    .unwrap();
    assert!(point.main_history && point.annotation.is_some());
    let mut unrelated = repository.clone();
    unrelated.main_refs[0].oid = "unrelated".into();
    let git = git.with_ancestry_path(Some("unrelated"), "remote", &[]);
    include_mainline_groups(
        &git,
        &mut groups,
        &main,
        &unrelated,
        Order::Newest,
        &mut metadata,
    )
    .unwrap();
    assert_eq!(groups.len(), 1);
}

fn fast_forward_git() -> FakeLaneBackend {
    tracked()
        .with_local_branches(&[("local", &["main"]), ("feature", &["feature/topic"])])
        .with_revset("main()", false, &["local"])
        .with_branch_names(
            "((all()) & branches()) - public()",
            false,
            &["feature/topic"],
        )
        .with_revset(
            "heads(((all()) & branches()) - public())",
            false,
            &["feature"],
        )
        .with_merge_base("local", "remote", Some("local"))
        .with_merge_base("remote", "remote", Some("remote"))
        .with_merge_base("feature", "remote", Some("remote"))
}

#[test]
fn mainline_selection_and_grouping_preserve_backend_failures() {
    for failure in [1, 2] {
        let git = FakeLaneBackend {
            unmerged_failure: Some(failure),
            ..fast_forward_git()
        };
        assert!(query_branchless_lane_selection(&git, "all()", false, &()).is_err());
    }
    let git = FakeLaneBackend {
        heads_error: true,
        ..fast_forward_git()
    };
    assert!(query_branchless_lane_selection(&git, "all()", false, &()).is_err());
    let git = FakeLaneBackend {
        heads_error: true,
        ..fast_forward_git().with_branch_names("((all()) & branches()) - public()", false, &[])
    };
    assert!(query_branchless_lane_selection(&git, "all()", false, &()).is_err());
    let git = FakeLaneBackend {
        unmerged_failure: Some(2),
        ..fast_forward_git()
    };
    assert!(query_plain_git_lane_selection(&git, &()).is_err());
    let git = tracked().with_merge_base("local", "remote", Some("local"));
    let (main, repository) = mainline_repository(&git, "main", "local").unwrap();
    let mut groups = Vec::new();
    let mut metadata = HashMap::new();
    assert!(
        include_mainline_groups(
            &git,
            &mut groups,
            &main,
            &repository,
            Order::Newest,
            &mut metadata
        )
        .is_err()
    );
    let git = git.with_ancestry_path(Some("local"), "remote", &["remote"]);
    assert!(
        include_mainline_groups(
            &git,
            &mut groups,
            &main,
            &repository,
            Order::Newest,
            &mut metadata
        )
        .is_err()
    );
}

#[test]
fn unrelated_features_and_failed_local_fork_queries_do_not_invent_mainline_ancestry() {
    for base in [None, Some("root")] {
        let git = tracked()
            .with_merge_base("local", "remote", Some("root"))
            .with_merge_base("remote", "remote", Some("remote"))
            .with_merge_base("feature", "remote", None)
            .with_merge_base("local", "feature", base);
        let (_, repository) = mainline_repository(&git, "main", "local").unwrap();
        let mut points =
            HashMap::from([("feature".into(), BranchPointRef::new("feature", ["topic"]))]);
        let mut git = git;
        if base.is_some() {
            git.unmerged_failure = Some(2);
        }
        let result = include_mainline_points(&git, "remote", &repository, &mut points);
        if base.is_some() {
            assert!(result.is_err());
        } else {
            result.unwrap();
        }
    }
    let git = tracked()
        .with_merge_base("local", "remote", Some("root"))
        .with_merge_base("remote", "remote", Some("remote"))
        .with_merge_base("feature", "remote", None);
    let (_, repository) = mainline_repository(&git, "main", "local").unwrap();
    let mut points = HashMap::from([("feature".into(), BranchPointRef::new("feature", ["topic"]))]);
    assert!(include_mainline_points(&git, "remote", &repository, &mut points).is_err());
}

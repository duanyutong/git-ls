use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use crate::cli::{Backend, ColourMode, Order, Palette, RuntimeOptions, Verbosity};
use crate::model::{BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane};

#[derive(Default)]
struct FakeLaneBackend {
    revsets: HashMap<(String, bool), Vec<String>>,
    branch_names: HashMap<(String, bool), Vec<String>>,
    local_branches: HashMap<String, Vec<String>>,
    current_head: Option<String>,
    current_branch: Option<String>,
    main_name: String,
    merge_bases: HashMap<(String, String), Option<String>>,
    ancestry_paths: HashMap<(Option<String>, String), Vec<String>>,
    metas: HashMap<String, CommitMeta>,
    metadata_requests: RefCell<Vec<Vec<String>>>,
}

impl FakeLaneBackend {
    fn with_revset(mut self, revset: &str, hidden: bool, oids: &[&str]) -> Self {
        self.revsets.insert(
            (revset.to_string(), hidden),
            oids.iter().map(|oid| (*oid).to_string()).collect(),
        );
        self
    }

    fn with_branch_names(mut self, revset: &str, hidden: bool, names: &[&str]) -> Self {
        self.branch_names.insert(
            (revset.to_string(), hidden),
            names.iter().map(|name| (*name).to_string()).collect(),
        );
        self
    }

    fn with_local_branches(mut self, branches: &[(&str, &[&str])]) -> Self {
        self.local_branches = branches
            .iter()
            .map(|(oid, names)| {
                (
                    (*oid).to_string(),
                    names.iter().map(|name| (*name).to_string()).collect(),
                )
            })
            .collect();
        self
    }

    fn with_head(mut self, head: Option<&str>, branch: Option<&str>) -> Self {
        self.current_head = head.map(ToOwned::to_owned);
        self.current_branch = branch.map(ToOwned::to_owned);
        self
    }

    fn with_main_name(mut self, main_name: &str) -> Self {
        self.main_name = main_name.to_string();
        self
    }

    fn with_merge_base(mut self, main_oid: &str, head_oid: &str, base_oid: Option<&str>) -> Self {
        self.merge_bases.insert(
            (main_oid.to_string(), head_oid.to_string()),
            base_oid.map(ToOwned::to_owned),
        );
        self
    }

    fn with_ancestry_path(mut self, base_oid: Option<&str>, head_oid: &str, path: &[&str]) -> Self {
        self.ancestry_paths.insert(
            (base_oid.map(ToOwned::to_owned), head_oid.to_string()),
            path.iter().map(|oid| (*oid).to_string()).collect(),
        );
        self
    }

    fn with_meta(mut self, oid: &str, subject: &str, timestamp: i64) -> Self {
        self.metas.insert(
            oid.to_string(),
            CommitMeta {
                oid: oid.to_string(),
                short_oid: oid.to_string(),
                subject: subject.to_string(),
                timestamp,
            },
        );
        self
    }

    fn metadata_requests(&self) -> Vec<Vec<String>> {
        self.metadata_requests.borrow().clone()
    }
}

fn missing_fixture(name: &str, key: impl std::fmt::Debug) -> GitLsError {
    GitLsError::TestFixture(format!("missing {name} fixture for {key:?}"))
}

fn insert_fake_meta(cache: &mut HashMap<String, CommitMeta>, alias: &str, meta: CommitMeta) {
    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
}

impl BranchlessQueries for FakeLaneBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.revsets
            .get(&(revset.to_string(), hidden))
            .cloned()
            .ok_or_else(|| missing_fixture("revset", (revset, hidden)))
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.branch_names
            .get(&(revset.to_string(), hidden))
            .cloned()
            .ok_or_else(|| missing_fixture("branch names", (revset, hidden)))
    }
}

impl RepositoryStateBackend for FakeLaneBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        Ok(self.local_branches.clone())
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        Ok((self.current_head.clone(), self.current_branch.clone()))
    }

    fn main_branch_name(&self) -> Result<String> {
        if self.main_name.is_empty() {
            Ok("main".to_string())
        } else {
            Ok(self.main_name.clone())
        }
    }
}

impl CommitMetadataBackend for FakeLaneBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        self.metadata_requests
            .borrow_mut()
            .push(oids.iter().map(|oid| (*oid).to_string()).collect());
        for oid in oids {
            let meta = self
                .metas
                .get(*oid)
                .cloned()
                .ok_or_else(|| missing_fixture("metadata", oid))?;
            insert_fake_meta(cache, oid, meta);
        }
        Ok(())
    }
}

impl AncestryBackend for FakeLaneBackend {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        self.merge_bases
            .get(&(main_oid.to_string(), head_oid.to_string()))
            .cloned()
            .ok_or_else(|| missing_fixture("merge base", (main_oid, head_oid)))
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        self.ancestry_paths
            .get(&(base_oid.map(ToOwned::to_owned), head_oid.to_string()))
            .cloned()
            .ok_or_else(|| missing_fixture("ancestry path", (base_oid, head_oid)))
    }
}

fn point(oid: &str, names: &[&str]) -> BranchPoint {
    BranchPoint {
        oid: oid.to_string(),
        names: names.iter().map(|name| (*name).to_string()).collect(),
        annotation: None,
    }
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

fn lane(oid: &str, base: Option<&str>, timestamp: i64, contains_current: bool) -> Lane {
    Lane {
        head_oid: oid.to_string(),
        base_oid: base.map(ToOwned::to_owned),
        branch_points: vec![point(oid, &[oid])],
        head_timestamp: timestamp,
        contains_current,
    }
}

fn lane_group(base: Option<&str>, main_distance: Option<usize>, lanes: Vec<Lane>) -> LaneGroup {
    LaneGroup {
        base_oid: base.map(ToOwned::to_owned),
        base_meta: None,
        main_distance,
        lanes,
    }
}

#[test]
fn creates_branch_revset() {
    assert_eq!(
        branch_revset("draft()"),
        "((draft()) & branches()) - public()"
    );
}

#[test]
fn maps_selected_branch_points_by_oid() {
    let mut branches = HashMap::new();
    branches.insert(
        "a".to_string(),
        vec!["zeta".to_string(), "alpha".to_string(), "main".to_string()],
    );
    branches.insert("b".to_string(), vec!["ignored".to_string()]);

    let selected = vec!["alpha".to_string(), "zeta".to_string()];
    let points = branch_points_by_oid(&selected, &branches);

    assert_eq!(points.len(), 1);
    assert_eq!(
        points.get("a").unwrap(),
        &BranchPointRef {
            oid: "a".to_string(),
            names: vec!["alpha".to_string(), "zeta".to_string()],
        }
    );
}

#[test]
fn orders_lanes_by_time_and_oid_without_current_promotion() {
    let lanes = vec![
        lane("older", Some("main"), 10, false),
        lane("current", Some("main"), 15, true),
        lane("newer-b", Some("main"), 20, false),
        lane("newer-a", Some("main"), 20, false),
    ];

    let newest: Vec<String> = ordered_lanes(lanes.clone(), Order::Newest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(newest, vec!["newer-a", "newer-b", "current", "older"]);

    let oldest: Vec<String> = ordered_lanes(lanes, Order::Oldest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(oldest, vec!["older", "current", "newer-a", "newer-b"]);
}

#[test]
fn orders_lanes_by_head_oid_when_timestamps_match() {
    let lanes = vec![
        lane("same-c", Some("main"), 20, false),
        lane("same-a", Some("main"), 20, false),
        lane("same-b", Some("main"), 20, false),
    ];

    let ordered: Vec<String> = ordered_lanes(lanes, Order::Newest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();

    assert_eq!(ordered, vec!["same-a", "same-b", "same-c"]);
}

#[test]
fn groups_lanes_by_base_time_without_current_promotion() {
    let lanes = vec![
        lane("a", Some("base-a"), 30, false),
        lane("b", Some("base-b"), 2, true),
        lane("c", Some("base-a"), 10, false),
    ];

    let groups = grouped_by_base(lanes.clone(), Order::Newest);

    assert_eq!(groups[0].0, Some("base-a".to_string()));
    assert_eq!(groups[0].1.len(), 2);
    assert_eq!(groups[1].0, Some("base-b".to_string()));
    assert_eq!(groups[1].1.len(), 1);

    let groups = grouped_by_base(lanes, Order::Oldest);

    assert_eq!(groups[0].0, Some("base-b".to_string()));
    assert_eq!(groups[0].1.len(), 1);
    assert_eq!(groups[1].0, Some("base-a".to_string()));
    assert_eq!(groups[1].1.len(), 2);
}

#[test]
fn builds_lane_groups_with_main_history_distances() {
    let git = FakeLaneBackend::default()
        .with_meta("old-main", "old base", 1_700_000_001)
        .with_ancestry_path(
            Some("old-main"),
            "main-oid",
            &["main-1", "main-2", "main-oid"],
        );
    let lanes = vec![
        lane("current", Some("main-oid"), 2, false),
        lane("old", Some("old-main"), 1, true),
    ];
    let mut cache = HashMap::new();

    let groups = build_lane_groups(&git, lanes, "main-oid", Order::Newest, &mut cache).unwrap();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].base_oid, Some("main-oid".to_string()));
    assert_eq!(groups[0].main_distance, Some(0));
    assert_eq!(groups[0].base_meta, None);
    assert_eq!(groups[1].base_oid, Some("old-main".to_string()));
    assert_eq!(groups[1].main_distance, Some(3));
    assert_eq!(
        groups[1]
            .base_meta
            .as_ref()
            .map(|meta| meta.subject.as_str()),
        Some("old base")
    );
}

#[test]
fn lane_group_order_prefers_main_distance_before_timestamp() {
    let mut groups = [
        lane_group(
            Some("far-main"),
            Some(3),
            vec![lane("newer", None, 100, false)],
        ),
        lane_group(
            Some("near-main"),
            Some(1),
            vec![lane("older", None, 1, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("near-main".to_string()));
    assert_eq!(groups[1].base_oid, Some("far-main".to_string()));
}

#[test]
fn lane_group_order_uses_timestamp_after_distance_ties() {
    let mut groups = [
        lane_group(
            Some("older"),
            None,
            vec![lane("older-head", None, 1, false)],
        ),
        lane_group(
            Some("newer"),
            None,
            vec![lane("newer-head", None, 100, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("newer".to_string()));
    assert_eq!(groups[1].base_oid, Some("older".to_string()));
}

#[test]
fn lane_group_order_uses_base_oid_after_timestamp_ties() {
    let mut groups = [
        lane_group(Some("base-b"), None, vec![lane("b", None, 10, false)]),
        lane_group(Some("base-a"), None, vec![lane("a", None, 10, false)]),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("base-a".to_string()));
    assert_eq!(groups[1].base_oid, Some("base-b".to_string()));
}

#[test]
fn counts_each_branch_point_from_previous_visible_stack_point() {
    let mut points_by_oid = HashMap::new();
    points_by_oid.insert(
        "a".to_string(),
        BranchPointRef {
            oid: "a".to_string(),
            names: vec!["feature/one".to_string()],
        },
    );
    points_by_oid.insert(
        "b".to_string(),
        BranchPointRef {
            oid: "b".to_string(),
            names: vec!["feature/two".to_string()],
        },
    );
    let lane_path = LanePath {
        head_oid: "b".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["a".to_string(), "b".to_string()],
    };

    let points = branch_points_for_path(&lane_path, &points_by_oid);

    assert_eq!(
        points,
        vec![
            BranchPointOnPath {
                point: BranchPointRef {
                    oid: "b".to_string(),
                    names: vec!["feature/two".to_string()],
                },
                commit_count: 1,
            },
            BranchPointOnPath {
                point: BranchPointRef {
                    oid: "a".to_string(),
                    names: vec!["feature/one".to_string()],
                },
                commit_count: 1,
            },
        ]
    );
}

#[test]
fn builds_lane_from_path_with_metadata_and_current_status() {
    let git = FakeLaneBackend::default()
        .with_meta("a", "first", 10)
        .with_meta("b", "second", 20);
    let points_by_oid = HashMap::from([
        (
            "a".to_string(),
            BranchPointRef {
                oid: "a".to_string(),
                names: vec!["feature/one".to_string()],
            },
        ),
        (
            "b".to_string(),
            BranchPointRef {
                oid: "b".to_string(),
                names: vec!["feature/two".to_string()],
            },
        ),
    ]);
    let lane_path = LanePath {
        head_oid: "b".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["a".to_string(), "b".to_string()],
    };
    let mut cache = HashMap::new();

    let lane = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        Some("feature/two"),
        None,
        Verbosity::Medium,
        &mut cache,
    )
    .unwrap()
    .unwrap();

    assert_eq!(lane.head_oid, "b");
    assert_eq!(lane.base_oid, Some("main-oid".to_string()));
    assert!(lane.contains_current);
    assert_eq!(
        lane.branch_points,
        vec![
            point_with_count_at("b", &["feature/two"], 1, "second", 20),
            point_with_count_at("a", &["feature/one"], 1, "first", 10),
        ]
    );
}

#[test]
fn includes_head_branch_point_when_ancestry_path_does_not_contain_it() {
    let points_by_oid = HashMap::from([(
        "head".to_string(),
        BranchPointRef {
            oid: "head".to_string(),
            names: vec!["feature/head".to_string()],
        },
    )]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: None,
        ancestry_path: vec!["root".to_string(), "parent".to_string()],
    };

    let points = branch_points_for_path(&lane_path, &points_by_oid);

    assert_eq!(
        points,
        vec![BranchPointOnPath {
            point: BranchPointRef {
                oid: "head".to_string(),
                names: vec!["feature/head".to_string()],
            },
            commit_count: 2,
        }]
    );
}

#[test]
fn builds_lanes_from_backend_facts() {
    let revset = "((draft()) & branches()) - public()";
    let git = FakeLaneBackend::default()
        .with_revset("main()", false, &["main-oid"])
        .with_branch_names(
            revset,
            false,
            &["feature/one", "feature/two", "chore/other"],
        )
        .with_local_branches(&[
            ("a", &["feature/one"]),
            ("b", &["feature/two"]),
            ("c", &["chore/other"]),
            ("main-oid", &["main"]),
        ])
        .with_revset(&format!("heads({revset})"), false, &["b", "c"])
        .with_head(Some("b"), Some("feature/two"))
        .with_main_name("main")
        .with_meta("b", "second", 1_700_000_002)
        .with_meta("c", "third", 1_700_000_001)
        .with_merge_base("main-oid", "b", Some("main-oid"))
        .with_ancestry_path(Some("main-oid"), "b", &["a", "b"])
        .with_merge_base("main-oid", "c", Some("main-oid"))
        .with_ancestry_path(Some("main-oid"), "c", &["c"]);
    let args = RuntimeOptions {
        revset: "draft()".to_string(),
        hidden: false,
        verbosity: Verbosity::Low,
        backend: Backend::Gix,
        order: Order::Newest,
        colour_mode: ColourMode::Never,
        palette: Palette::Classic,
    };
    let mut cache = HashMap::new();

    let BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
    } = build_lanes(&git, &args, &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(repository.main_name, "main");
    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].head_oid, "b");
    assert!(lanes[0].contains_current);
    assert_eq!(
        lanes[0].branch_points,
        vec![point("b", &["feature/two"]), point("a", &["feature/one"])]
    );
    assert_eq!(lanes[1].head_oid, "c");
    assert!(!lanes[1].contains_current);
    assert_eq!(
        git.metadata_requests(),
        vec![vec!["b".to_string(), "c".to_string()]]
    );
}

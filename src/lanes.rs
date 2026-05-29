use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::backend::{GitBackend, get_commit_meta};
use crate::cli::{Order, RuntimeOptions, Verbosity};
use crate::error::{GitLsError, Result};
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, LaneGroup,
    RepositorySnapshot,
};

fn branch_revset(user_revset: &str) -> String {
    format!("(({user_revset}) & branches()) - public()")
}

fn branch_points_by_oid(
    branch_names: &[String],
    branch_oid_map: &HashMap<String, Vec<String>>,
) -> HashMap<String, BranchPointRef> {
    let selected: HashSet<&str> = branch_names.iter().map(String::as_str).collect();
    let mut result = HashMap::new();

    for (oid, names) in branch_oid_map {
        let mut point_names: Vec<String> = names
            .iter()
            .filter(|name| selected.contains(name.as_str()))
            .cloned()
            .collect();
        if point_names.is_empty() {
            continue;
        }
        point_names.sort();

        result.insert(
            oid.clone(),
            BranchPointRef {
                oid: oid.clone(),
                names: point_names,
            },
        );
    }

    result
}

fn build_branch_point<G: GitBackend + ?Sized>(
    git: &G,
    point: &BranchPointRef,
    commit_count: usize,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BranchPoint> {
    let annotation = if verbosity.includes_metadata() {
        Some(BranchAnnotation {
            meta: get_commit_meta(git, &point.oid, meta_cache)?,
            commit_count,
        })
    } else {
        None
    };

    Ok(BranchPoint {
        oid: point.oid.clone(),
        names: point.names.clone(),
        annotation,
    })
}

fn build_lane<G: GitBackend + ?Sized>(
    git: &G,
    head_oid: &str,
    main_oid: &str,
    points_by_oid: &HashMap<String, BranchPointRef>,
    current_branch: Option<&str>,
    head: Option<&str>,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let base_oid = git.merge_base(main_oid, head_oid)?;
    let path = git.ancestry_path(base_oid.as_deref(), head_oid)?;
    let mut branch_points = Vec::new();
    let mut previous_branch_point_index = None;

    for (index, oid) in path.iter().enumerate() {
        if let Some(point) = points_by_oid.get(oid) {
            let commit_count = previous_branch_point_index
                .map_or(index + 1, |previous| index.saturating_sub(previous));
            branch_points.push(build_branch_point(
                git,
                point,
                commit_count,
                verbosity,
                meta_cache,
            )?);
            previous_branch_point_index = Some(index);
        }
    }

    if branch_points.is_empty()
        && let Some(point) = points_by_oid.get(head_oid)
    {
        let commit_count = path
            .iter()
            .position(|oid| oid == head_oid)
            .map_or_else(|| path.len().max(1), |index| index + 1);
        branch_points.push(build_branch_point(
            git,
            point,
            commit_count,
            verbosity,
            meta_cache,
        )?);
    }
    if branch_points.is_empty() {
        return Ok(None);
    }

    branch_points.reverse();
    let head_meta = get_commit_meta(git, head_oid, meta_cache)?;
    let contains_current = branch_points.iter().any(|point| {
        current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
            || head.is_some_and(|head| point.oid == head)
    });

    Ok(Some(Lane {
        head_oid: head_meta.oid,
        base_oid,
        branch_points,
        head_timestamp: head_meta.timestamp,
        contains_current,
    }))
}

pub(crate) fn build_lanes<G: GitBackend + ?Sized>(
    git: &G,
    args: &RuntimeOptions,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BuiltLanes> {
    let revset = branch_revset(&args.revset);
    let main_oids = git.query_revset("main()", args.hidden)?;
    if main_oids.len() != 1 {
        return Err(GitLsError::ambiguous_main_revset(main_oids.len()));
    }
    let main_oid = main_oids[0].clone();

    let branch_names = git.query_branch_names(&revset, args.hidden)?;
    if branch_names.is_empty() {
        let (head, current_branch) = git.current_head_and_branch()?;
        return Ok(BuiltLanes::Empty {
            main_oid,
            repository: RepositorySnapshot {
                current_branch,
                head,
                main_name: git.main_branch_name()?,
            },
        });
    }

    let branch_oid_map = git.local_branches_by_oid()?;
    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    let heads_revset = format!("heads({revset})");
    let head_oids = git.query_revset(&heads_revset, args.hidden)?;
    let (head, current_branch) = git.current_head_and_branch()?;
    let main_name = git.main_branch_name()?;

    let mut meta_refs: Vec<&str> = head_oids.iter().map(String::as_str).collect();
    if args.verbosity.includes_metadata() {
        meta_refs.extend(points_by_oid.keys().map(String::as_str));
    }
    meta_refs.sort_unstable();
    meta_refs.dedup();
    git.cache_commit_metas(&meta_refs, meta_cache)?;

    let mut lanes = Vec::new();
    for head_oid in head_oids {
        if let Some(lane) = build_lane(
            git,
            &head_oid,
            &main_oid,
            &points_by_oid,
            current_branch.as_deref(),
            head.as_deref(),
            args.verbosity,
            meta_cache,
        )? {
            lanes.push(lane);
        }
    }

    Ok(BuiltLanes::Populated {
        lanes,
        main_oid,
        repository: RepositorySnapshot {
            current_branch,
            head,
            main_name,
        },
    })
}

pub(crate) fn ordered_lanes(mut lanes: Vec<Lane>, order: Order) -> Vec<Lane> {
    lanes.sort_by(|lhs, rhs| {
        match order {
            Order::Newest => rhs.head_timestamp.cmp(&lhs.head_timestamp),
            Order::Oldest => lhs.head_timestamp.cmp(&rhs.head_timestamp),
        }
        .then_with(|| lhs.head_oid.cmp(&rhs.head_oid))
    });
    lanes
}

fn grouped_by_base(lanes: Vec<Lane>, order: Order) -> Vec<(Option<String>, Vec<Lane>)> {
    let mut groups: HashMap<Option<String>, Vec<Lane>> = HashMap::new();
    for lane in lanes {
        groups.entry(lane.base_oid.clone()).or_default().push(lane);
    }

    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|lhs, rhs| {
        lane_group_timestamp_order(&lhs.1, &rhs.1, order).then_with(|| {
            lhs.0
                .as_deref()
                .unwrap_or("")
                .cmp(rhs.0.as_deref().unwrap_or(""))
        })
    });
    groups
}

pub(crate) fn build_lane_groups<G: GitBackend + ?Sized>(
    git: &G,
    lanes: Vec<Lane>,
    main_oid: &str,
    order: Order,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<LaneGroup>> {
    let mut groups = Vec::new();
    for (base_oid, lanes) in grouped_by_base(lanes, order) {
        let base_meta = match base_oid.as_deref() {
            Some(base_oid) if base_oid != main_oid => {
                Some(get_commit_meta(git, base_oid, meta_cache)?)
            }
            _ => None,
        };
        let main_distance = match base_oid.as_deref() {
            Some(base_oid) if base_oid == main_oid => Some(0),
            Some(base_oid) => {
                let path = git.ancestry_path(Some(base_oid), main_oid)?;
                if path.is_empty() {
                    None
                } else {
                    Some(path.len())
                }
            }
            None => None,
        };

        groups.push(LaneGroup {
            base_oid,
            base_meta,
            main_distance,
            lanes,
        });
    }

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, order));
    Ok(groups)
}

fn lane_group_order(lhs: &LaneGroup, rhs: &LaneGroup, order: Order) -> Ordering {
    match (lhs.main_distance, rhs.main_distance) {
        (Some(lhs_distance), Some(rhs_distance)) => lhs_distance
            .cmp(&rhs_distance)
            .then_with(|| lane_group_fallback_order(lhs, rhs, order)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => lane_group_fallback_order(lhs, rhs, order),
    }
}

fn lane_group_timestamp(lanes: &[Lane], order: Order) -> i64 {
    match order {
        Order::Newest => lanes
            .iter()
            .map(|lane| lane.head_timestamp)
            .max()
            .unwrap_or(i64::MIN),
        Order::Oldest => lanes
            .iter()
            .map(|lane| lane.head_timestamp)
            .min()
            .unwrap_or(i64::MAX),
    }
}

fn lane_group_timestamp_order(lhs: &[Lane], rhs: &[Lane], order: Order) -> Ordering {
    let lhs_timestamp = lane_group_timestamp(lhs, order);
    let rhs_timestamp = lane_group_timestamp(rhs, order);
    match order {
        Order::Newest => rhs_timestamp.cmp(&lhs_timestamp),
        Order::Oldest => lhs_timestamp.cmp(&rhs_timestamp),
    }
}

fn lane_group_fallback_order(lhs: &LaneGroup, rhs: &LaneGroup, order: Order) -> Ordering {
    lane_group_timestamp_order(&lhs.lanes, &rhs.lanes, order).then_with(|| {
        lhs.base_oid
            .as_deref()
            .unwrap_or("")
            .cmp(rhs.base_oid.as_deref().unwrap_or(""))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::cli::{Backend, ColourMode, Order, Palette, RuntimeOptions, Verbosity};
    use crate::model::{
        BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane,
    };
    use crate::test_support::MockGit;

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

    fn meta(oid: &str, subject: &str) -> CommitMeta {
        CommitMeta {
            oid: oid.to_string(),
            short_oid: oid.to_string(),
            subject: subject.to_string(),
            timestamp: 0,
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
        let git = MockGit::default()
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "old-main",
                ],
                "old-main\x001700000001\x00old base\x1e",
            )
            .with(
                &[
                    "rev-list",
                    "--reverse",
                    "--ancestry-path",
                    "old-main..main-oid",
                ],
                "main-1\nmain-2\nmain-oid",
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
    fn counts_each_branch_point_from_previous_visible_stack_point() {
        let git = MockGit::default()
            .with(&["merge-base", "main-oid", "b"], "main-oid")
            .with(
                &["rev-list", "--reverse", "--ancestry-path", "main-oid..b"],
                "a\nb",
            );
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
        let mut cache = HashMap::from([
            ("a".to_string(), meta("a", "first")),
            ("b".to_string(), meta("b", "second")),
        ]);

        let lane = build_lane(
            &git,
            "b",
            "main-oid",
            &points_by_oid,
            None,
            None,
            Verbosity::Medium,
            &mut cache,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            lane.branch_points,
            vec![
                point_with_count_at("b", &["feature/two"], 1, "second", 0),
                point_with_count_at("a", &["feature/one"], 1, "first", 0),
            ]
        );
    }

    #[test]
    fn builds_lanes_with_mocked_git_boundary() {
        let revset = "((draft()) & branches()) - public()";
        let git = MockGit::default()
            .with(&["branchless", "query", "-r", "main()"], "main-oid")
            .with(
                &["branchless", "query", "-b", revset],
                "feature/one\nfeature/two\nchore/other",
            )
            .with(
                &[
                    "for-each-ref",
                    "--format=%(objectname)%00%(refname:short)",
                    "refs/heads",
                ],
                "a\0feature/one\nb\0feature/two\nc\0chore/other\nmain-oid\0main",
            )
            .with(
                &["branchless", "query", "-r", &format!("heads({revset})")],
                "b\nc",
            )
            .with(
                &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
                "b\nfeature/two",
            )
            .with(&["config", "--get", "branchless.core.mainBranch"], "")
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "b",
                    "c",
                ],
                "b\x001700000002\x00second\x1e\nc\x001700000001\x00third\x1e",
            )
            .with(&["merge-base", "main-oid", "b"], "main-oid")
            .with(
                &["rev-list", "--reverse", "--ancestry-path", "main-oid..b"],
                "a\nb",
            )
            .with(&["merge-base", "main-oid", "c"], "main-oid")
            .with(
                &["rev-list", "--reverse", "--ancestry-path", "main-oid..c"],
                "c",
            );
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

        let calls = git.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.first().is_some_and(|arg| arg == "for-each-ref"))
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call
                    .get(2)
                    .is_some_and(|arg| arg == "--format=%H%x00%ct%x00%s%x1e"))
                .count(),
            1
        );
    }
}

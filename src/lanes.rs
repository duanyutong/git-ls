use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::backend::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, GitBackend, RepositoryStateBackend,
    get_commit_meta,
};
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct LanePath {
    head_oid: String,
    base_oid: Option<String>,
    ancestry_path: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BranchPointOnPath {
    point: BranchPointRef,
    commit_count: usize,
}

enum LaneSelection {
    Empty {
        main_oid: String,
        repository: RepositorySnapshot,
    },
    Populated {
        main_oid: String,
        head_oids: Vec<String>,
        points_by_oid: HashMap<String, BranchPointRef>,
        repository: RepositorySnapshot,
    },
}

fn query_lane_selection<G>(git: &G, user_revset: &str, hidden: bool) -> Result<LaneSelection>
where
    G: BranchlessQueries + RepositoryStateBackend + ?Sized,
{
    let revset = branch_revset(user_revset);
    let main_oids = git.query_revset("main()", hidden)?;
    if main_oids.len() != 1 {
        return Err(GitLsError::ambiguous_main_revset(main_oids.len()));
    }
    let main_oid = main_oids[0].clone();

    let branch_names = git.query_branch_names(&revset, hidden)?;
    let (head, current_branch) = git.current_head_and_branch()?;
    let main_name = git.main_branch_name()?;
    let repository = RepositorySnapshot {
        current_branch,
        head,
        main_name,
    };

    if branch_names.is_empty() {
        return Ok(LaneSelection::Empty {
            main_oid,
            repository,
        });
    }

    let branch_oid_map = git.local_branches_by_oid()?;
    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    let heads_revset = format!("heads({revset})");
    let head_oids = git.query_revset(&heads_revset, hidden)?;

    Ok(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
    })
}

fn prefetch_lane_metadata<G: CommitMetadataBackend + ?Sized>(
    git: &G,
    head_oids: &[String],
    points_by_oid: &HashMap<String, BranchPointRef>,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let mut meta_refs: Vec<&str> = head_oids.iter().map(String::as_str).collect();
    if verbosity.includes_metadata() {
        meta_refs.extend(points_by_oid.keys().map(String::as_str));
    }
    meta_refs.sort_unstable();
    meta_refs.dedup();
    git.cache_commit_metas(&meta_refs, meta_cache)
}

fn collect_lane_path<G: AncestryBackend + ?Sized>(
    git: &G,
    main_oid: &str,
    head_oid: &str,
) -> Result<LanePath> {
    let base_oid = git.merge_base(main_oid, head_oid)?;
    let ancestry_path = git.ancestry_path(base_oid.as_deref(), head_oid)?;
    Ok(LanePath {
        head_oid: head_oid.to_string(),
        base_oid,
        ancestry_path,
    })
}

fn branch_points_for_path(
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
) -> Vec<BranchPointOnPath> {
    let mut branch_points = Vec::new();
    let mut previous_branch_point_index = None;

    for (index, oid) in lane_path.ancestry_path.iter().enumerate() {
        if let Some(point) = points_by_oid.get(oid) {
            let commit_count = previous_branch_point_index
                .map_or(index + 1, |previous| index.saturating_sub(previous));
            branch_points.push(BranchPointOnPath {
                point: point.clone(),
                commit_count,
            });
            previous_branch_point_index = Some(index);
        }
    }

    if branch_points.is_empty()
        && let Some(point) = points_by_oid.get(&lane_path.head_oid)
    {
        let commit_count = lane_path
            .ancestry_path
            .iter()
            .position(|oid| oid == &lane_path.head_oid)
            .map_or_else(|| lane_path.ancestry_path.len().max(1), |index| index + 1);
        branch_points.push(BranchPointOnPath {
            point: point.clone(),
            commit_count,
        });
    }

    branch_points.reverse();
    branch_points
}

fn build_branch_point<G: CommitMetadataBackend + ?Sized>(
    git: &G,
    branch_point: &BranchPointOnPath,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BranchPoint> {
    let point = &branch_point.point;
    let annotation = if verbosity.includes_metadata() {
        Some(BranchAnnotation {
            meta: get_commit_meta(git, &point.oid, meta_cache)?,
            commit_count: branch_point.commit_count,
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

fn build_lane_from_path<G: CommitMetadataBackend + ?Sized>(
    git: &G,
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
    current_branch: Option<&str>,
    head: Option<&str>,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let branch_points = branch_points_for_path(lane_path, points_by_oid);
    if branch_points.is_empty() {
        return Ok(None);
    }

    let branch_points = branch_points
        .iter()
        .map(|point| build_branch_point(git, point, verbosity, meta_cache))
        .collect::<Result<Vec<_>>>()?;
    let head_meta = get_commit_meta(git, &lane_path.head_oid, meta_cache)?;
    let contains_current = branch_points.iter().any(|point| {
        current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
            || head.is_some_and(|head| point.oid == head)
    });

    Ok(Some(Lane {
        head_oid: head_meta.oid,
        base_oid: lane_path.base_oid.clone(),
        branch_points,
        head_timestamp: head_meta.timestamp,
        contains_current,
    }))
}

fn build_lane<G: AncestryBackend + CommitMetadataBackend + ?Sized>(
    git: &G,
    head_oid: &str,
    main_oid: &str,
    points_by_oid: &HashMap<String, BranchPointRef>,
    current_branch: Option<&str>,
    head: Option<&str>,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let lane_path = collect_lane_path(git, main_oid, head_oid)?;
    build_lane_from_path(
        git,
        &lane_path,
        points_by_oid,
        current_branch,
        head,
        verbosity,
        meta_cache,
    )
}

pub(crate) fn build_lanes<G: GitBackend + ?Sized>(
    git: &G,
    args: &RuntimeOptions,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BuiltLanes> {
    let (main_oid, head_oids, points_by_oid, repository) =
        match query_lane_selection(git, &args.revset, args.hidden)? {
            LaneSelection::Empty {
                main_oid,
                repository,
            } => {
                return Ok(BuiltLanes::Empty {
                    main_oid,
                    repository,
                });
            }
            LaneSelection::Populated {
                main_oid,
                head_oids,
                points_by_oid,
                repository,
            } => (main_oid, head_oids, points_by_oid, repository),
        };

    prefetch_lane_metadata(git, &head_oids, &points_by_oid, args.verbosity, meta_cache)?;

    let mut lanes = Vec::new();
    for head_oid in head_oids {
        if let Some(lane) = build_lane(
            git,
            &head_oid,
            &main_oid,
            &points_by_oid,
            repository.current_branch.as_deref(),
            repository.head.as_deref(),
            args.verbosity,
            meta_cache,
        )? {
            lanes.push(lane);
        }
    }

    Ok(BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
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
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::cli::{Backend, ColourMode, Order, Palette, RuntimeOptions, Verbosity};
    use crate::model::{
        BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane,
    };

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

        fn with_merge_base(
            mut self,
            main_oid: &str,
            head_oid: &str,
            base_oid: Option<&str>,
        ) -> Self {
            self.merge_bases.insert(
                (main_oid.to_string(), head_oid.to_string()),
                base_oid.map(ToOwned::to_owned),
            );
            self
        }

        fn with_ancestry_path(
            mut self,
            base_oid: Option<&str>,
            head_oid: &str,
            path: &[&str],
        ) -> Self {
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
}

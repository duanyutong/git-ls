use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::backend::{GitBackend, get_commit_meta};
use crate::cli::{EffectiveArgs, Order, Verbosity};
use crate::error::{GitLsError, Result};
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, LaneGroup,
    RepositorySnapshot,
};

pub(crate) fn branch_revset(user_revset: &str) -> String {
    format!("(({user_revset}) & branches()) - public()")
}

pub(crate) fn branch_points_by_oid(
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

pub(crate) fn build_lane<G: GitBackend + ?Sized>(
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
    args: &EffectiveArgs,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BuiltLanes> {
    let revset = branch_revset(&args.revset);
    let main_oids = git.query_revset("main()", args.hidden)?;
    if main_oids.len() != 1 {
        return Err(GitLsError::AmbiguousMainRevset {
            count: main_oids.len(),
        });
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

pub(crate) fn grouped_by_base(lanes: Vec<Lane>, order: Order) -> Vec<(Option<String>, Vec<Lane>)> {
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

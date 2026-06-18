use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::backend::{
    AncestryBackend, BranchlessQueries, GitBackend, RepositoryStateBackend, get_commit_meta,
};
use crate::cli::{DEFAULT_REVSET, Order, RuntimeOptions, Verbosity};
use crate::error::{GitLsError, Result};
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, LaneGroup,
    RepositorySnapshot, RewrittenCommit,
};

fn branch_revset(user_revset: &str) -> String {
    format!("(({user_revset}) & branches()) - public()")
}

fn debug_log(enabled: bool, message: fmt::Arguments<'_>) {
    if enabled {
        eprintln!("git-ls debug: {message}");
    }
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

        result.insert(oid.clone(), BranchPointRef::new(oid, point_names));
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
struct RewrittenCommitRef {
    oid: String,
    replacement_oid: String,
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
        detect_rewritten_commits: bool,
    },
}

fn query_lane_selection(
    git: &dyn GitBackend,
    user_revset: &str,
    hidden: bool,
    debug: bool,
) -> Result<LaneSelection> {
    if uses_plain_git_fallback(user_revset) && !hidden {
        debug_log(debug, format_args!("selection: using plain Git default"));
        return query_plain_git_lane_selection(git, debug);
    }

    debug_log(
        debug,
        format_args!("selection: trying branchless revset={user_revset:?} hidden={hidden}"),
    );
    if let Some(selection) = query_branchless_lane_selection(git, user_revset, hidden, debug)? {
        return Ok(selection);
    }

    debug_log(debug, format_args!("selection: using plain Git fallback"));
    query_plain_git_lane_selection(git, debug)
}

fn uses_plain_git_fallback(user_revset: &str) -> bool {
    user_revset.trim() == DEFAULT_REVSET
}

fn query_branchless_lane_selection(
    git: &dyn GitBackend,
    user_revset: &str,
    hidden: bool,
    debug: bool,
) -> Result<Option<LaneSelection>> {
    let revset = branch_revset(user_revset);
    let main_oids = match BranchlessQueries::query_revset(git, "main()", hidden) {
        Ok(main_oids) => main_oids,
        Err(error) if uses_plain_git_fallback(user_revset) => {
            debug_log(
                debug,
                format_args!("branchless selection: main() query failed: {error}"),
            );
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if main_oids.len() != 1 {
        return Err(GitLsError::ambiguous_main_revset(main_oids.len()));
    }
    let main_oid = main_oids[0].clone();

    let branch_names = match BranchlessQueries::query_branch_names(git, &revset, hidden) {
        Ok(branch_names) => branch_names,
        Err(error) if uses_plain_git_fallback(user_revset) => {
            debug_log(
                debug,
                format_args!("branchless selection: branch query failed: {error}"),
            );
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if branch_names.is_empty() {
        if uses_plain_git_fallback(user_revset) {
            debug_log(
                debug,
                format_args!("branchless selection: default revset selected no branches"),
            );
            return Ok(None);
        }
        let (head, current_branch) = RepositoryStateBackend::current_head_and_branch(git)?;
        let main_name = RepositoryStateBackend::main_branch_name(git)?;
        let repository = RepositorySnapshot::new(current_branch, head, main_name);
        return Ok(Some(LaneSelection::Empty {
            main_oid,
            repository,
        }));
    }

    let (head, current_branch) = RepositoryStateBackend::current_head_and_branch(git)?;
    let main_name = RepositoryStateBackend::main_branch_name(git)?;
    let repository = RepositorySnapshot::new(current_branch, head, main_name);
    let branch_oid_map = RepositoryStateBackend::local_branches_by_oid(git)?;
    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    let heads_revset = format!("heads({revset})");
    let head_oids = match BranchlessQueries::query_revset(git, &heads_revset, hidden) {
        Ok(head_oids) => head_oids,
        Err(error) if uses_plain_git_fallback(user_revset) => {
            debug_log(
                debug,
                format_args!("branchless selection: heads query failed: {error}"),
            );
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    debug_log(
        debug,
        format_args!(
            "branchless selection: branches={} heads={} main={}",
            branch_names.len(),
            head_oids.len(),
            main_oid
        ),
    );

    Ok(Some(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
        detect_rewritten_commits: true,
    }))
}

fn query_plain_git_lane_selection(git: &dyn GitBackend, debug: bool) -> Result<LaneSelection> {
    let branch_oid_map = RepositoryStateBackend::local_branches_by_oid(git)?;
    debug_log(
        debug,
        format_args!(
            "plain Git fallback: local branch tips={}",
            branch_oid_map.len()
        ),
    );
    let configured_main_name = RepositoryStateBackend::main_branch_name(git)?;
    let (main_name, main_oid) = plain_git_main_branch(&branch_oid_map, &configured_main_name)?;
    debug_log(
        debug,
        format_args!("plain Git fallback: main branch={main_name} oid={main_oid}"),
    );
    let (head, current_branch) = RepositoryStateBackend::current_head_and_branch(git)?;
    let repository = RepositorySnapshot::new(current_branch, head, main_name.clone());
    let branch_names = plain_git_branch_names(git, &branch_oid_map, &main_name, &main_oid)?;
    debug_log(
        debug,
        format_args!(
            "plain Git fallback: selected local branches={}",
            branch_names.len()
        ),
    );

    if branch_names.is_empty() {
        return Ok(LaneSelection::Empty {
            main_oid,
            repository,
        });
    }

    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    let head_oids = plain_git_stack_heads(git, points_by_oid.keys())?;
    debug_log(
        debug,
        format_args!("plain Git fallback: stack heads={}", head_oids.len()),
    );
    debug_assert!(!head_oids.is_empty());

    Ok(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
        detect_rewritten_commits: false,
    })
}

fn plain_git_main_branch(
    branch_oid_map: &HashMap<String, Vec<String>>,
    configured_main_name: &str,
) -> Result<(String, String)> {
    let candidates = plain_git_main_branch_candidates(configured_main_name);
    for candidate in &candidates {
        if let Some(oid) = oid_for_branch(branch_oid_map, candidate) {
            return Ok((candidate.clone(), oid.to_string()));
        }
    }

    Err(GitLsError::plain_git_main_branch_not_found(
        candidates.join(", "),
    ))
}

fn plain_git_main_branch_candidates(configured_main_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for candidate in [configured_main_name, "main", "master", "trunk"] {
        if !candidate.is_empty() && !candidates.iter().any(|existing| existing == candidate) {
            candidates.push(candidate.to_string());
        }
    }
    candidates
}

fn oid_for_branch<'a>(
    branch_oid_map: &'a HashMap<String, Vec<String>>,
    branch_name: &str,
) -> Option<&'a str> {
    branch_oid_map
        .iter()
        .find(|(_, names)| names.iter().any(|name| name == branch_name))
        .map(|(oid, _)| oid.as_str())
}

fn plain_git_branch_names(
    git: &dyn GitBackend,
    branch_oid_map: &HashMap<String, Vec<String>>,
    main_name: &str,
    main_oid: &str,
) -> Result<Vec<String>> {
    let mut branch_names = Vec::new();
    for (oid, names) in branch_oid_map {
        if oid == main_oid || is_ancestor(git, oid, main_oid)? {
            continue;
        }

        branch_names.extend(
            names
                .iter()
                .filter(|name| name.as_str() != main_name)
                .cloned(),
        );
    }
    branch_names.sort();
    branch_names.dedup();
    Ok(branch_names)
}

fn plain_git_stack_heads<'a>(
    git: &dyn GitBackend,
    selected_oids: impl IntoIterator<Item = &'a String>,
) -> Result<Vec<String>> {
    let mut oids: Vec<String> = selected_oids.into_iter().cloned().collect();
    oids.sort();
    oids.dedup();

    let mut heads = Vec::new();
    for oid in &oids {
        let mut is_intermediate = false;
        for other in oids.iter().filter(|other| *other != oid) {
            if is_ancestor(git, oid, other)? {
                is_intermediate = true;
                break;
            }
        }
        if !is_intermediate {
            heads.push(oid.clone());
        }
    }

    Ok(heads)
}

fn is_ancestor(git: &dyn GitBackend, ancestor_oid: &str, descendant_oid: &str) -> Result<bool> {
    if ancestor_oid == descendant_oid {
        return Ok(true);
    }

    Ok(
        AncestryBackend::merge_base(git, ancestor_oid, descendant_oid)?.as_deref()
            == Some(ancestor_oid),
    )
}

fn prefetch_lane_metadata(
    git: &dyn GitBackend,
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

fn collect_lane_path(git: &dyn GitBackend, main_oid: &str, head_oid: &str) -> Result<LanePath> {
    let base_oid = AncestryBackend::merge_base(git, main_oid, head_oid)?;
    let ancestry_path = AncestryBackend::ancestry_path(git, base_oid.as_deref(), head_oid)?;
    Ok(LanePath {
        head_oid: head_oid.to_string(),
        base_oid,
        ancestry_path,
    })
}

fn rewritten_commits_for_path(
    git: &dyn GitBackend,
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
) -> Result<Vec<RewrittenCommitRef>> {
    let mut rewritten_commits = Vec::new();
    for oid in &lane_path.ancestry_path {
        if points_by_oid.contains_key(oid) {
            continue;
        }

        let current_revset = format!("current({oid})");
        let current_oids = BranchlessQueries::query_revset(git, &current_revset, true)?;
        let [replacement_oid] = current_oids.as_slice() else {
            continue;
        };
        if replacement_oid != oid && points_by_oid.contains_key(replacement_oid) {
            rewritten_commits.push(RewrittenCommitRef {
                oid: oid.clone(),
                replacement_oid: replacement_oid.clone(),
            });
        }
    }
    Ok(rewritten_commits)
}

fn branch_points_for_path(
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
    rewritten_commits: &[RewrittenCommitRef],
) -> Vec<BranchPointOnPath> {
    let mut branch_points = Vec::new();
    let rewritten_oids: HashSet<&str> = rewritten_commits
        .iter()
        .map(|commit| commit.oid.as_str())
        .collect();
    let mut commits_since_previous_branch = 0;

    for oid in &lane_path.ancestry_path {
        if rewritten_oids.contains(oid.as_str()) {
            continue;
        }
        commits_since_previous_branch += 1;
        if let Some(point) = points_by_oid.get(oid) {
            branch_points.push(BranchPointOnPath {
                point: point.clone(),
                commit_count: commits_since_previous_branch,
            });
            commits_since_previous_branch = 0;
        }
    }

    if branch_points.is_empty()
        && let Some(point) = points_by_oid.get(&lane_path.head_oid)
    {
        let commit_count = lane_path
            .ancestry_path
            .iter()
            .filter(|oid| !rewritten_oids.contains(oid.as_str()))
            .count()
            .max(1);
        branch_points.push(BranchPointOnPath {
            point: point.clone(),
            commit_count,
        });
    }

    branch_points.reverse();
    branch_points
}

fn build_rewritten_commits(
    git: &dyn GitBackend,
    rewritten_refs: &[RewrittenCommitRef],
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<RewrittenCommit>> {
    rewritten_refs
        .iter()
        .rev()
        .map(|commit| {
            let meta = get_commit_meta(git, &commit.oid, meta_cache)?;
            let replacement = get_commit_meta(git, &commit.replacement_oid, meta_cache)?;
            Ok(RewrittenCommit::new(meta, replacement))
        })
        .collect()
}

fn build_branch_point(
    git: &dyn GitBackend,
    branch_point: &BranchPointOnPath,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BranchPoint> {
    let point = &branch_point.point;
    let annotation = if verbosity.includes_metadata() {
        Some(BranchAnnotation::new(
            get_commit_meta(git, &point.oid, meta_cache)?,
            branch_point.commit_count,
        ))
    } else {
        None
    };

    Ok(BranchPoint::new(
        point.oid.clone(),
        point.names.clone(),
        annotation,
    ))
}

#[derive(Clone, Copy)]
struct LaneBuildContext<'a> {
    current_branch: Option<&'a str>,
    head: Option<&'a str>,
    verbosity: Verbosity,
    detect_rewritten_commits: bool,
    debug: bool,
}

fn build_lane_from_path(
    git: &dyn GitBackend,
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
    context: LaneBuildContext<'_>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let branch_points = branch_points_for_path(lane_path, points_by_oid, &[]);
    if branch_points.is_empty() {
        return Ok(None);
    }

    let rewritten_refs = if context.detect_rewritten_commits {
        debug_log(
            context.debug,
            format_args!(
                "lane: detecting rewritten commits along {} ancestry commits",
                lane_path.ancestry_path.len()
            ),
        );
        let rewritten_refs = rewritten_commits_for_path(git, lane_path, points_by_oid)?;
        debug_log(
            context.debug,
            format_args!("lane: detected rewritten commits={}", rewritten_refs.len()),
        );
        rewritten_refs
    } else {
        Vec::new()
    };
    let branch_points = branch_points_for_path(lane_path, points_by_oid, &rewritten_refs);
    let branch_points = branch_points
        .iter()
        .map(|point| build_branch_point(git, point, context.verbosity, meta_cache))
        .collect::<Result<Vec<_>>>()?;
    let rewritten_commits = build_rewritten_commits(git, &rewritten_refs, meta_cache)?;
    let head_meta = get_commit_meta(git, &lane_path.head_oid, meta_cache)?;
    let contains_current = branch_points.iter().any(|point| {
        context
            .current_branch
            .is_some_and(|branch| point.names.iter().any(|name| name == branch))
            || context.head.is_some_and(|head| point.oid == head)
    });

    let head_timestamp = head_meta.timestamp;
    Ok(Some(Lane::new_with_rewritten_commits(
        head_meta.oid,
        lane_path.base_oid.clone(),
        branch_points,
        rewritten_commits,
        head_timestamp,
        contains_current,
    )))
}

fn build_lane(
    git: &dyn GitBackend,
    head_oid: &str,
    main_oid: &str,
    points_by_oid: &HashMap<String, BranchPointRef>,
    context: LaneBuildContext<'_>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    debug_log(
        context.debug,
        format_args!("lane: collecting ancestry path main={main_oid} head={head_oid}"),
    );
    let lane_path = collect_lane_path(git, main_oid, head_oid)?;
    build_lane_from_path(git, &lane_path, points_by_oid, context, meta_cache)
}

pub(crate) fn build_lanes(
    git: &dyn GitBackend,
    args: &RuntimeOptions,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BuiltLanes> {
    let (main_oid, head_oids, points_by_oid, repository, detect_rewritten_commits) =
        match query_lane_selection(git, &args.revset, args.hidden, args.debug)? {
            LaneSelection::Empty {
                main_oid,
                repository,
            } => {
                return Ok(BuiltLanes::empty(main_oid, repository));
            }
            LaneSelection::Populated {
                main_oid,
                head_oids,
                points_by_oid,
                repository,
                detect_rewritten_commits,
            } => (
                main_oid,
                head_oids,
                points_by_oid,
                repository,
                detect_rewritten_commits,
            ),
        };

    debug_log(
        args.debug,
        format_args!(
            "selection: populated main={} heads={} branch_points={} rewritten_detection={}",
            main_oid,
            head_oids.len(),
            points_by_oid.len(),
            detect_rewritten_commits
        ),
    );
    prefetch_lane_metadata(git, &head_oids, &points_by_oid, args.verbosity, meta_cache)?;

    let mut lanes = Vec::new();
    for head_oid in head_oids {
        if let Some(lane) = build_lane(
            git,
            &head_oid,
            &main_oid,
            &points_by_oid,
            LaneBuildContext {
                current_branch: repository.current_branch.as_deref(),
                head: repository.head.as_deref(),
                verbosity: args.verbosity,
                detect_rewritten_commits,
                debug: args.debug,
            },
            meta_cache,
        )? {
            lanes.push(lane);
        }
    }

    Ok(BuiltLanes::populated(lanes, main_oid, repository))
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

pub(crate) fn build_lane_groups(
    git: &dyn GitBackend,
    lanes: Vec<Lane>,
    main_oid: &str,
    order: Order,
    debug: bool,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<LaneGroup>> {
    debug_log(
        debug,
        format_args!("lane groups: building groups for {} lanes", lanes.len()),
    );
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
                debug_log(
                    debug,
                    format_args!(
                        "lane groups: measuring main distance base={base_oid} main={main_oid}"
                    ),
                );
                let path = AncestryBackend::ancestry_path(git, Some(base_oid), main_oid)?;
                if path.is_empty() {
                    None
                } else {
                    Some(path.len())
                }
            }
            None => None,
        };

        groups.push(LaneGroup::new(base_oid, base_meta, main_distance, lanes));
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
mod tests;

use super::observer::{AnalysisObserver, AnalysisStage};
use super::selection::{LaneSelection, query_lane_selection};
use crate::backend::{AncestryBackend, BranchlessQueries, GitBackend, get_commit_meta};
use crate::cli::{RuntimeOptions, Verbosity};
use crate::error::Result;
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, RewrittenCommit,
};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LanePath {
    pub(super) head_oid: String,
    pub(super) base_oid: Option<String>,
    pub(super) ancestry_path: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RewrittenCommitRef {
    pub(super) oid: String,
    pub(super) replacement_oid: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BranchPointOnPath {
    pub(super) point: BranchPointRef,
    pub(super) commit_count: usize,
}

pub(super) fn prefetch_lane_metadata(
    git: &dyn GitBackend,
    head_oids: &[String],
    points_by_oid: &HashMap<String, BranchPointRef>,
    verbosity: Verbosity,
    meta_cache: &mut HashMap<String, CommitMeta>,
    observer: &dyn AnalysisObserver,
) -> Result<()> {
    let _perf = AnalysisStage::new(observer, "metadata_prefetch");
    let mut meta_refs: Vec<&str> = head_oids.iter().map(String::as_str).collect();
    if verbosity.includes_metadata() {
        meta_refs.extend(points_by_oid.keys().map(String::as_str));
    }
    meta_refs.sort_unstable();
    meta_refs.dedup();
    git.cache_commit_metas(&meta_refs, meta_cache)
}

pub(super) fn collect_lane_path(
    git: &dyn GitBackend,
    main_oid: &str,
    head_oid: &str,
    observer: &dyn AnalysisObserver,
) -> Result<LanePath> {
    let perf = AnalysisStage::new(observer, "lane_path");
    let base_oid = AncestryBackend::merge_base(git, main_oid, head_oid)?;
    let ancestry_path = AncestryBackend::ancestry_path(git, base_oid.as_deref(), head_oid)?;
    perf.detail(format_args!(
        "head={head_oid} base={} returned_commits={}",
        base_oid.as_deref().unwrap_or("none"),
        ancestry_path.len()
    ));
    Ok(LanePath {
        head_oid: head_oid.to_string(),
        base_oid,
        ancestry_path,
    })
}

pub(super) fn rewritten_commits_for_path(
    git: &dyn GitBackend,
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
    observer: &dyn AnalysisObserver,
) -> Result<Vec<RewrittenCommitRef>> {
    let _perf = AnalysisStage::new(observer, "rewrites");
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

pub(super) fn branch_points_for_path(
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

pub(super) fn build_rewritten_commits(
    git: &dyn GitBackend,
    rewritten_refs: &[RewrittenCommitRef],
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<RewrittenCommit>> {
    let mut oids: Vec<_> = rewritten_refs
        .iter()
        .flat_map(|commit| [commit.oid.as_str(), commit.replacement_oid.as_str()])
        .collect();
    oids.sort_unstable();
    oids.dedup();
    if !oids.is_empty() {
        git.cache_commit_metas(&oids, meta_cache)?;
    }
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

pub(super) fn build_branch_point(
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

    let mut result = BranchPoint::new(point.oid.clone(), point.names.clone(), annotation);
    result.commit_count = branch_point.commit_count;
    Ok(result)
}

#[derive(Clone, Copy)]
pub(super) struct LaneBuildContext<'a> {
    pub(super) current_branch: Option<&'a str>,
    pub(super) head: Option<&'a str>,
    pub(super) verbosity: Verbosity,
    pub(super) detect_rewritten_commits: bool,
    pub(super) observer: &'a dyn AnalysisObserver,
}

pub(super) fn build_lane_from_path(
    git: &dyn GitBackend,
    lane_path: &LanePath,
    points_by_oid: &HashMap<String, BranchPointRef>,
    context: LaneBuildContext<'_>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let observer = context.observer;
    let branch_points = branch_points_for_path(lane_path, points_by_oid, &[]);
    if branch_points.is_empty() {
        return Ok(None);
    }

    let rewritten_refs = if context.detect_rewritten_commits {
        observer.progress(format_args!(
            "lane: detecting rewritten commits along {} ancestry commits",
            lane_path.ancestry_path.len()
        ));
        let rewritten_refs = rewritten_commits_for_path(git, lane_path, points_by_oid, observer)?;
        observer.progress(format_args!(
            "lane: detected rewritten commits={}",
            rewritten_refs.len()
        ));
        rewritten_refs
    } else {
        Vec::new()
    };
    let branch_points = if rewritten_refs.is_empty() {
        branch_points
    } else {
        branch_points_for_path(lane_path, points_by_oid, &rewritten_refs)
    };
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

pub(super) fn build_lane(
    git: &dyn GitBackend,
    head_oid: &str,
    main_oid: &str,
    points_by_oid: &HashMap<String, BranchPointRef>,
    context: LaneBuildContext<'_>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let observer = context.observer;
    observer.progress(format_args!(
        "lane: collecting ancestry path main={main_oid} head={head_oid}"
    ));
    let lane_path = collect_lane_path(git, main_oid, head_oid, observer)?;
    build_lane_from_path(git, &lane_path, points_by_oid, context, meta_cache)
}

pub(crate) fn build_lanes(
    git: &dyn GitBackend,
    args: &RuntimeOptions,
    meta_cache: &mut HashMap<String, CommitMeta>,
    observer: &dyn AnalysisObserver,
) -> Result<BuiltLanes> {
    let (main_oid, head_oids, points_by_oid, repository, detect_rewritten_commits) =
        match query_lane_selection(git, &args.revset, args.hidden, observer)? {
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

    observer.progress(format_args!(
        "selection: populated main={} heads={} branch_points={} rewritten_detection={}",
        main_oid,
        head_oids.len(),
        points_by_oid.len(),
        detect_rewritten_commits
    ));
    prefetch_lane_metadata(
        git,
        &head_oids,
        &points_by_oid,
        args.verbosity,
        meta_cache,
        observer,
    )?;

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
                observer,
            },
            meta_cache,
        )? {
            lanes.push(lane);
        }
    }

    observer.count("lanes", lanes.len());
    Ok(BuiltLanes::populated(lanes, main_oid, repository))
}

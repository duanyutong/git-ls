use super::observer::{AnalysisObserver, AnalysisStage};
use crate::backend::{BranchlessQueries, GitBackend, RepositoryStateBackend};
use crate::cli::DEFAULT_REVSET;
use crate::error::{GitLsError, Result};
use crate::model::{BranchPointRef, RepositorySnapshot};
use std::collections::{HashMap, HashSet};
pub(super) fn branch_revset(user_revset: &str) -> String {
    format!("(({user_revset}) & branches()) - public()")
}

pub(super) fn branch_points_by_oid(
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

pub(super) enum LaneSelection {
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

pub(super) fn query_lane_selection(
    git: &dyn GitBackend,
    user_revset: &str,
    hidden: bool,
    observer: &dyn AnalysisObserver,
) -> Result<LaneSelection> {
    let _perf = AnalysisStage::new(observer, "selection");
    if uses_plain_git_fallback(user_revset) && !hidden {
        observer.progress(format_args!("selection: using plain Git default"));
        return query_plain_git_lane_selection(git, observer);
    }

    observer.progress(format_args!(
        "selection: trying branchless revset={user_revset:?} hidden={hidden}"
    ));
    if let Some(selection) = query_branchless_lane_selection(git, user_revset, hidden, observer)? {
        return Ok(selection);
    }

    observer.progress(format_args!("selection: using plain Git fallback"));
    query_plain_git_lane_selection(git, observer)
}

pub(super) fn uses_plain_git_fallback(user_revset: &str) -> bool {
    user_revset.trim() == DEFAULT_REVSET
}

pub(super) fn query_branchless_lane_selection(
    git: &dyn GitBackend,
    user_revset: &str,
    hidden: bool,
    observer: &dyn AnalysisObserver,
) -> Result<Option<LaneSelection>> {
    let _perf = AnalysisStage::new(observer, "branchless_selection");
    let revset = branch_revset(user_revset);
    let main_oids = match BranchlessQueries::query_revset(git, "main()", hidden) {
        Ok(main_oids) => main_oids,
        Err(error) if uses_plain_git_fallback(user_revset) => {
            observer.progress(format_args!(
                "branchless selection: main() query failed: {error}"
            ));
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
            observer.progress(format_args!(
                "branchless selection: branch query failed: {error}"
            ));
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if branch_names.is_empty() {
        if uses_plain_git_fallback(user_revset) {
            observer.progress(format_args!(
                "branchless selection: default revset selected no branches"
            ));
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
            observer.progress(format_args!(
                "branchless selection: heads query failed: {error}"
            ));
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    observer.progress(format_args!(
        "branchless selection: branches={} heads={} main={}",
        branch_names.len(),
        head_oids.len(),
        main_oid
    ));

    Ok(Some(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
        detect_rewritten_commits: true,
    }))
}

pub(super) fn query_plain_git_lane_selection(
    git: &dyn GitBackend,
    observer: &dyn AnalysisObserver,
) -> Result<LaneSelection> {
    let _perf = AnalysisStage::new(observer, "plain_selection");
    let branch_oid_map = {
        let _perf = AnalysisStage::new(observer, "repository_snapshot");
        RepositoryStateBackend::local_branches_by_oid(git)?
    };
    observer.count("local_tips", branch_oid_map.len());
    observer.count(
        "local_branches",
        branch_oid_map.values().map(Vec::len).sum(),
    );
    observer.progress(format_args!(
        "plain Git fallback: local branch tips={}",
        branch_oid_map.len()
    ));
    let configured_main_name = RepositoryStateBackend::main_branch_name(git)?;
    let (main_name, main_oid) = plain_git_main_branch(&branch_oid_map, &configured_main_name)?;
    observer.progress(format_args!(
        "plain Git fallback: main branch={main_name} oid={main_oid}"
    ));
    let (head, current_branch) = RepositoryStateBackend::current_head_and_branch(git)?;
    let repository = RepositorySnapshot::new(current_branch, head, main_name.clone());
    let branch_names =
        plain_git_branch_names(git, &branch_oid_map, &main_name, &main_oid, observer)?;
    observer.progress(format_args!(
        "plain Git fallback: selected local branches={}",
        branch_names.len()
    ));

    if branch_names.is_empty() {
        return Ok(LaneSelection::Empty {
            main_oid,
            repository,
        });
    }

    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    observer.count("selected_tips", points_by_oid.len());
    observer.count("selected_branches", branch_names.len());
    let head_oids = plain_git_stack_heads(git, &main_oid, points_by_oid.keys(), observer)?;
    observer.count("heads", head_oids.len());
    observer.progress(format_args!(
        "plain Git fallback: stack heads={}",
        head_oids.len()
    ));

    Ok(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
        detect_rewritten_commits: false,
    })
}

pub(crate) fn plain_git_main_branch(
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

pub(super) fn plain_git_main_branch_candidates(configured_main_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for candidate in [configured_main_name, "main", "master", "trunk"] {
        if !candidate.is_empty() && !candidates.iter().any(|existing| existing == candidate) {
            candidates.push(candidate.to_string());
        }
    }
    candidates
}

pub(super) fn oid_for_branch<'a>(
    branch_oid_map: &'a HashMap<String, Vec<String>>,
    branch_name: &str,
) -> Option<&'a str> {
    branch_oid_map
        .iter()
        .find(|(_, names)| names.iter().any(|name| name == branch_name))
        .map(|(oid, _)| oid.as_str())
}

pub(super) fn plain_git_branch_names(
    git: &dyn GitBackend,
    branch_oid_map: &HashMap<String, Vec<String>>,
    main_name: &str,
    main_oid: &str,
    observer: &dyn AnalysisObserver,
) -> Result<Vec<String>> {
    let _perf = AnalysisStage::new(observer, "branch_filter");
    let tips: Vec<_> = branch_oid_map.keys().cloned().collect();
    let unmerged: HashSet<_> = git.unmerged_tips(main_oid, &tips)?.into_iter().collect();
    let mut branch_names = Vec::new();
    for (oid, names) in branch_oid_map {
        if !unmerged.contains(oid) {
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

pub(super) fn plain_git_stack_heads<'a>(
    git: &dyn GitBackend,
    main_oid: &str,
    selected_oids: impl IntoIterator<Item = &'a String>,
    observer: &dyn AnalysisObserver,
) -> Result<Vec<String>> {
    let _perf = AnalysisStage::new(observer, "stack_heads");
    let mut oids: Vec<String> = selected_oids.into_iter().cloned().collect();
    oids.sort();
    oids.dedup();

    let mut heads = git.independent_heads(main_oid, &oids)?;
    heads.sort();
    heads.dedup();
    Ok(heads)
}

use std::collections::HashMap;

use crate::error::{GitLsError, Result};
use crate::model::CommitMeta;

use super::metadata::{
    GIT_SHOW_COMMIT_META_ARG, git_show_commit_records, insert_commit_meta, missing_commit_aliases,
    parse_shell_commit_meta,
};
use super::traits::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, GitCommand, RepositoryStateBackend,
};

impl<T: GitCommand + ?Sized> BranchlessQueries for T {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(self, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(self, revset, hidden)
    }
}

impl<T: GitCommand + ?Sized> CommitMetadataBackend for T {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        shell_cache_commit_metas(self, oids, cache)
    }
}

impl<T: GitCommand + ?Sized> RepositoryStateBackend for T {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        shell_local_branches_by_oid(self)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        shell_current_head_and_branch(self)
    }

    fn main_branch_name(&self) -> Result<String> {
        shell_main_branch_name(self)
    }
}

impl<T: GitCommand + ?Sized> AncestryBackend for T {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        shell_merge_base(self, main_oid, head_oid)
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        shell_ancestry_path(self, base_oid, head_oid)
    }
}

pub(super) fn shell_query_revset<G: GitCommand + ?Sized>(
    git: &G,
    revset: &str,
    hidden: bool,
) -> Result<Vec<String>> {
    let mut args = vec!["branchless", "query", "-r"];
    if hidden {
        args.push("--hidden");
    }
    args.push(revset);
    Ok(lines(&git.run(&args, false)?))
}

pub(super) fn shell_query_branch_names<G: GitCommand + ?Sized>(
    git: &G,
    revset: &str,
    hidden: bool,
) -> Result<Vec<String>> {
    let mut args = vec!["branchless", "query", "-b"];
    if hidden {
        args.push("--hidden");
    }
    args.push(revset);
    Ok(lines(&git.run(&args, false)?))
}

pub(super) fn lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn shell_cache_commit_metas<G: GitCommand + ?Sized>(
    git: &G,
    oids: &[&str],
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let missing = missing_commit_aliases(oids, cache);
    if missing.is_empty() {
        return Ok(());
    }

    let mut args = vec!["show", "-s", GIT_SHOW_COMMIT_META_ARG, "--no-walk=unsorted"];
    args.extend(missing.iter().copied());

    let output = git.run(&args, false)?;
    let records = git_show_commit_records(&output);

    if records.len() != missing.len() {
        return Err(GitLsError::unexpected_git_show(missing.join(", ")));
    }

    for (alias, record) in missing.into_iter().zip(records) {
        let meta = parse_shell_commit_meta(alias, record)?;
        insert_commit_meta(cache, alias, meta);
    }

    Ok(())
}

fn shell_local_branches_by_oid<G: GitCommand + ?Sized>(
    git: &G,
) -> Result<HashMap<String, Vec<String>>> {
    let output = git.run(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:short)",
            "refs/heads",
        ],
        false,
    )?;
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let Some((oid, branch)) = line.split_once('\0') else {
            continue;
        };
        result
            .entry(oid.to_string())
            .or_default()
            .push(branch.to_string());
    }
    Ok(result)
}

fn shell_current_head_and_branch<G: GitCommand + ?Sized>(
    git: &G,
) -> Result<(Option<String>, Option<String>)> {
    let output = git.run(&["rev-parse", "HEAD", "--abbrev-ref", "HEAD"], true)?;
    let mut values = lines(&output).into_iter();
    let head = values.next();
    let branch = values.next().filter(|branch| branch != "HEAD");
    Ok((head, branch))
}

fn shell_main_branch_name<G: GitCommand + ?Sized>(git: &G) -> Result<String> {
    let output = git.run(&["config", "--get", "branchless.core.mainBranch"], true)?;
    Ok(non_empty(&output).unwrap_or_else(|| "main".to_string()))
}

pub(crate) fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn shell_merge_base<G: GitCommand + ?Sized>(
    git: &G,
    main_oid: &str,
    head_oid: &str,
) -> Result<Option<String>> {
    let output = git.run(&["merge-base", main_oid, head_oid], true)?;
    Ok(non_empty(&output))
}

fn shell_ancestry_path<G: GitCommand + ?Sized>(
    git: &G,
    base_oid: Option<&str>,
    head_oid: &str,
) -> Result<Vec<String>> {
    let output = match base_oid {
        Some(base_oid) => git.run(
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                &format!("{base_oid}..{head_oid}"),
            ],
            false,
        )?,
        None => git.run(&["rev-list", "--reverse", head_oid], false)?,
    };
    Ok(lines(&output))
}

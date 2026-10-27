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

    fn query_rewrites(
        &self,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        shell_query_rewrites(self, oids, targets)
    }

    fn query_rewrites_in_path(
        &self,
        base: Option<&str>,
        head: &str,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        shell_path_rewrites(self, base, head, oids, targets)
    }
}

pub(super) fn shell_path_rewrites<G: GitCommand + ?Sized>(
    git: &G,
    base: Option<&str>,
    head: &str,
    oids: &[String],
    targets: &[String],
) -> Result<HashMap<String, String>> {
    if oids.is_empty() {
        return Ok(HashMap::new());
    }
    let range = base.map_or_else(
        || format!("ancestors({head})"),
        |base| format!("(range({base}, {head}) - {base})"),
    );
    let mut targets = targets.to_vec();
    targets.sort_unstable();
    let expression = format!("current(({range}) - ({}))", targets.join(" | "));
    if !targets.is_empty() && expression.len() <= 8192 {
        let current = shell_query_revset(git, &expression, true)?;
        if !current.iter().any(|id| targets.contains(id)) {
            return Ok(HashMap::new());
        }
    }
    shell_query_rewrites(git, oids, &targets)
}

pub(super) fn shell_query_rewrites<G: GitCommand + ?Sized>(
    git: &G,
    oids: &[String],
    targets: &[String],
) -> Result<HashMap<String, String>> {
    let mut result = HashMap::new();
    for batch in oids.chunks(128) {
        let mut pending = vec![batch];
        while let Some(group) = pending.pop() {
            let expression = format!("current({})", group.join(" | "));
            let current = shell_query_revset(git, &expression, true)?;
            if !current.iter().any(|oid| targets.contains(oid)) {
                continue;
            }
            if let [oid] = group {
                if let [replacement] = current.as_slice()
                    && replacement != oid
                {
                    result.insert(oid.clone(), replacement.clone());
                }
            } else {
                let (left, right) = group.split_at(group.len() / 2);
                pending.push(right);
                pending.push(left);
            }
        }
    }
    Ok(result)
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
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        super::batch::unmerged(self, main, tips)
    }
    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        super::batch::independent(self, main, tips)
    }
    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        super::batch::distances(self, main, bases)
    }

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        shell_merge_base(self, main_oid, head_oid)
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        shell_ancestry_path(self, base_oid, head_oid)
    }
}

pub(super) fn parse_config_records(output: &str) -> HashMap<String, String> {
    output
        .split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let (key, value) = record.split_once('\n').unwrap_or((record, "true"));
            (key.to_ascii_lowercase(), value.to_string())
        })
        .collect()
}

pub(super) fn shell_query_revset<G: GitCommand + ?Sized>(
    git: &G,
    revset: &str,
    hidden: bool,
) -> Result<Vec<String>> {
    let _perf = crate::diagnostics::operation("branchless_revset");
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
    let _perf = crate::diagnostics::operation("branchless_branches");
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
    let _perf = crate::diagnostics::operation("metadata");
    let missing = missing_commit_aliases(oids, cache);
    if missing.is_empty() {
        return Ok(());
    }

    // Bound argument size independently of the number of local branches.
    for batch in missing.chunks(256) {
        let mut args = vec!["show", "-s", GIT_SHOW_COMMIT_META_ARG, "--no-walk=unsorted"];
        args.extend(batch.iter().copied());

        let output = git.run(&args, false)?;
        let records = git_show_commit_records(&output);

        if records.len() != batch.len() {
            return Err(GitLsError::unexpected_git_show(batch.join(", ")));
        }

        for (alias, record) in batch.iter().copied().zip(records) {
            let meta = parse_shell_commit_meta(alias, record)?;
            insert_commit_meta(cache, alias, meta);
        }
    }

    Ok(())
}

fn shell_local_branches_by_oid<G: GitCommand + ?Sized>(
    git: &G,
) -> Result<HashMap<String, Vec<String>>> {
    let output = git.run(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:lstrip=2)",
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
    let output = git.run(&["rev-parse", "HEAD", "--symbolic-full-name", "HEAD"], true)?;
    let mut values = lines(&output).into_iter();
    let head = values.next();
    let branch = values
        .next()
        .filter(|branch| branch != "HEAD")
        .map(|branch| {
            branch
                .strip_prefix("refs/heads/")
                .unwrap_or(&branch)
                .to_string()
        });
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
    let _perf = crate::diagnostics::operation("merge_base");
    let output = git.run(&["merge-base", main_oid, head_oid], true)?;
    Ok(non_empty(&output))
}

fn shell_ancestry_path<G: GitCommand + ?Sized>(
    git: &G,
    base_oid: Option<&str>,
    head_oid: &str,
) -> Result<Vec<String>> {
    let _perf = crate::diagnostics::operation("ancestry_path");
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

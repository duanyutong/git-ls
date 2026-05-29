use std::collections::{HashMap, HashSet};
use std::process::Command;

use gix::bstr::ByteSlice as _;

use crate::error::{GitLsError, Result};
use crate::model::{CommitMeta, display_short_oid};

pub(crate) trait GitCommand {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String>;
}

#[derive(Debug, Default)]
pub(crate) struct ProcessGit;

impl GitCommand for ProcessGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(GitLsError::GitExec)?;

        if !output.status.success() && !allow_failure {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let fallback = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if detail.is_empty() { fallback } else { detail };
            return Err(GitLsError::GitCommand {
                args: args.join(" "),
                detail,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string())
    }
}

pub(crate) trait GitBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()>;
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>>;
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)>;
    fn main_branch_name(&self) -> Result<String>;
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>>;
    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>>;
}

impl<T: GitCommand + ?Sized> GitBackend for T {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(self, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(self, revset, hidden)
    }

    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        shell_cache_commit_metas(self, oids, cache)
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        shell_local_branches_by_oid(self)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        shell_current_head_and_branch(self)
    }

    fn main_branch_name(&self) -> Result<String> {
        shell_main_branch_name(self)
    }

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        shell_merge_base(self, main_oid, head_oid)
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        shell_ancestry_path(self, base_oid, head_oid)
    }
}

fn shell_query_revset<G: GitCommand + ?Sized>(
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

fn shell_query_branch_names<G: GitCommand + ?Sized>(
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

pub(crate) fn lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn get_commit_meta<G: GitBackend + ?Sized>(
    git: &G,
    oid: &str,
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<CommitMeta> {
    if let Some(meta) = cache.get(oid) {
        return Ok(meta.clone());
    }

    git.cache_commit_metas(&[oid], cache)?;
    cache
        .get(oid)
        .cloned()
        .ok_or_else(|| GitLsError::UnexpectedGitShow {
            oid: oid.to_string(),
        })
}

fn shell_cache_commit_metas<G: GitCommand + ?Sized>(
    git: &G,
    oids: &[&str],
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let mut seen = HashSet::new();
    let missing: Vec<&str> = oids
        .iter()
        .copied()
        .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "show",
        "-s",
        "--format=%H%x00%ct%x00%s%x1e",
        "--no-walk=unsorted",
    ];
    args.extend(missing.iter().copied());

    let output = git.run(&args, false)?;
    let records: Vec<&str> = output
        .split('\x1e')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .map(|record| record.strip_suffix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .collect();

    if records.len() != missing.len() {
        return Err(GitLsError::UnexpectedGitShow {
            oid: missing.join(", "),
        });
    }

    for (alias, record) in missing.into_iter().zip(records) {
        shell_cache_commit_meta(alias, record, cache)?;
    }

    Ok(())
}

pub(crate) fn shell_cache_commit_meta(
    alias: &str,
    record: &str,
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let parts: Vec<&str> = record.splitn(3, '\0').collect();
    if parts.len() != 3 {
        return Err(GitLsError::UnexpectedGitShow {
            oid: alias.to_string(),
        });
    }

    let meta = CommitMeta {
        oid: parts[0].to_string(),
        short_oid: display_short_oid(parts[0]),
        timestamp: parts[1]
            .parse()
            .map_err(|source| GitLsError::InvalidCommitTimestamp {
                oid: alias.to_string(),
                source,
            })?,
        subject: parts[2].to_string(),
    };

    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
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

pub(crate) struct GixBackend {
    repo: gix::Repository,
    command: ProcessGit,
}

impl GixBackend {
    pub(crate) fn discover() -> Result<Self> {
        Self::discover_from(".")
    }

    pub(crate) fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut repo = gix::discover_with_environment_overrides(directory)
            .map_err(|source| gix_error("discover repository", source))?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        Ok(Self {
            repo,
            command: ProcessGit,
        })
    }

    fn object_id(oid: &str) -> Result<gix::ObjectId> {
        oid.parse::<gix::ObjectId>()
            .map_err(|source| GitLsError::InvalidObjectId {
                oid: oid.to_string(),
                detail: source.to_string(),
            })
    }

    fn commit_meta(&self, alias: &str) -> Result<CommitMeta> {
        let oid = Self::object_id(alias)?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|source| gix_error("find commit", source))?;
        let full_oid = commit.id().detach().to_string();
        let subject = commit
            .message()
            .map_err(|source| gix_error("read commit message", source))?
            .summary()
            .to_str_lossy()
            .into_owned();
        let timestamp = commit
            .time()
            .map_err(|source| gix_error("read commit timestamp", source))?
            .seconds;
        let short_oid = display_short_oid(&full_oid);

        Ok(CommitMeta {
            oid: full_oid,
            short_oid,
            subject,
            timestamp,
        })
    }

    fn is_descendant_of(
        &self,
        commit: gix::ObjectId,
        ancestor: gix::ObjectId,
        cache: &mut HashMap<gix::ObjectId, bool>,
    ) -> Result<bool> {
        if commit == ancestor {
            return Ok(true);
        }
        if let Some(result) = cache.get(&commit) {
            return Ok(*result);
        }

        let commit_object = self
            .repo
            .find_commit(commit)
            .map_err(|source| gix_error("find ancestry commit", source))?;
        for parent in commit_object.parent_ids() {
            let parent = parent.detach();
            if parent == ancestor || self.is_descendant_of(parent, ancestor, cache)? {
                cache.insert(commit, true);
                return Ok(true);
            }
        }

        cache.insert(commit, false);
        Ok(false)
    }
}

impl GitBackend for GixBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(&self.command, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(&self.command, revset, hidden)
    }

    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let missing: Vec<&str> = oids
            .iter()
            .copied()
            .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
            .collect();
        for alias in missing {
            let meta = self.commit_meta(alias)?;
            if alias != meta.oid {
                cache.insert(alias.to_string(), meta.clone());
            }
            cache.insert(meta.oid.clone(), meta);
        }
        Ok(())
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for reference in self
            .repo
            .references()
            .map_err(|source| gix_error("open references", source))?
            .local_branches()
            .map_err(|source| gix_error("iterate local branches", source))?
        {
            let reference = reference.map_err(|source| gix_error("read local branch", source))?;
            let Some(id) = reference.try_id() else {
                continue;
            };
            result
                .entry(id.detach().to_string())
                .or_default()
                .push(reference.name().shorten().to_str_lossy().into_owned());
        }
        Ok(result)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        let Ok(head) = self.repo.head_id() else {
            return Ok((None, None));
        };
        let branch = self
            .repo
            .head_name()
            .map_err(|source| gix_error("read HEAD name", source))?
            .map(|name| name.shorten().to_str_lossy().into_owned());
        Ok((Some(head.detach().to_string()), branch))
    }

    fn main_branch_name(&self) -> Result<String> {
        Ok(self
            .repo
            .config_snapshot()
            .string("branchless.core.mainBranch")
            .map_or_else(
                || "main".to_string(),
                |value| value.to_str_lossy().into_owned(),
            ))
    }

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        let main_oid = Self::object_id(main_oid)?;
        let head_oid = Self::object_id(head_oid)?;
        match self.repo.merge_base(main_oid, head_oid) {
            Ok(base) => Ok(Some(base.detach().to_string())),
            Err(gix::repository::merge_base::Error::NotFound { .. }) => Ok(None),
            Err(source) => Err(gix_error("find merge base", source)),
        }
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        let head_oid = Self::object_id(head_oid)?;
        let mut walk =
            self.repo
                .rev_walk([head_oid])
                .sorting(gix::revision::walk::Sorting::ByCommitTime(
                    gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
                ));
        let base_oid = base_oid.map(Self::object_id).transpose()?;
        if let Some(base_oid) = base_oid {
            walk = walk.with_hidden([base_oid]);
        }

        let mut descendant_cache = HashMap::new();
        let mut path = Vec::new();
        for info in walk
            .all()
            .map_err(|source| gix_error("walk revisions", source))?
        {
            let oid = info
                .map_err(|source| gix_error("read revision walk entry", source))?
                .id;
            let include = match base_oid {
                Some(base) => self.is_descendant_of(oid, base, &mut descendant_cache)?,
                None => true,
            };
            if include {
                path.push(oid.to_string());
            }
        }
        path.reverse();
        Ok(path)
    }
}

fn gix_error(context: &'static str, source: impl std::fmt::Display) -> GitLsError {
    GitLsError::Gix {
        context,
        detail: source.to_string(),
    }
}

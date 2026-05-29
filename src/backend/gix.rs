use std::collections::HashMap;

use gix::bstr::ByteSlice as _;

use crate::error::{GitLsError, Result};
use crate::model::CommitMeta;

use super::metadata::{insert_commit_meta, missing_commit_aliases};
use super::process::ProcessGit;
use super::shell::{shell_query_branch_names, shell_query_revset};
use super::traits::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, RepositoryStateBackend,
};

/// Repository backend that uses `gix` for native Git data and shells out only
/// for `git-branchless` revset queries, whose semantics are external to `gix`.
pub(crate) struct GixBackend {
    repo: gix::Repository,
    command: ProcessGit,
}

impl GixBackend {
    pub(crate) fn discover() -> Result<Self> {
        Self::discover_from(".")
    }

    pub(super) fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut repo = gix::discover_with_environment_overrides(directory)
            .map_err(|source| GitLsError::gix("discover repository", source))?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        Ok(Self {
            repo,
            command: ProcessGit,
        })
    }

    pub(super) fn object_id(oid: &str) -> Result<gix::ObjectId> {
        oid.parse::<gix::ObjectId>()
            .map_err(|source| GitLsError::invalid_object_id(oid, source))
    }

    fn commit_meta(&self, alias: &str) -> Result<CommitMeta> {
        let oid = Self::object_id(alias)?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|source| GitLsError::gix("find commit", source))?;
        let full_oid = commit.id().detach().to_string();
        let subject = commit
            .message()
            .map_err(|source| GitLsError::gix("read commit message", source))?
            .summary()
            .to_str_lossy()
            .into_owned();
        let timestamp = commit
            .time()
            .map_err(|source| GitLsError::gix("read commit timestamp", source))?
            .seconds;
        Ok(CommitMeta::new(full_oid, timestamp, subject))
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
            .map_err(|source| GitLsError::gix("find ancestry commit", source))?;
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

impl BranchlessQueries for GixBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(&self.command, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(&self.command, revset, hidden)
    }
}

impl CommitMetadataBackend for GixBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        for alias in missing_commit_aliases(oids, cache) {
            let meta = self.commit_meta(alias)?;
            insert_commit_meta(cache, alias, meta);
        }
        Ok(())
    }
}

impl RepositoryStateBackend for GixBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for reference in self
            .repo
            .references()
            .map_err(|source| GitLsError::gix("open references", source))?
            .local_branches()
            .map_err(|source| GitLsError::gix("iterate local branches", source))?
        {
            let reference =
                reference.map_err(|source| GitLsError::gix("read local branch", source))?;
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
            .map_err(|source| GitLsError::gix("read HEAD name", source))?
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
}

impl AncestryBackend for GixBackend {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        let main_oid = Self::object_id(main_oid)?;
        let head_oid = Self::object_id(head_oid)?;
        match self.repo.merge_base(main_oid, head_oid) {
            Ok(base) => Ok(Some(base.detach().to_string())),
            Err(gix::repository::merge_base::Error::NotFound { .. }) => Ok(None),
            Err(source) => Err(GitLsError::gix("find merge base", source)),
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
            .map_err(|source| GitLsError::gix("walk revisions", source))?
        {
            let oid = info
                .map_err(|source| GitLsError::gix("read revision walk entry", source))?
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

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

mod graph;

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
    parents: RefCell<HashMap<gix::ObjectId, Vec<gix::ObjectId>>>,
    selection: RefCell<Option<graph::TipAnalysis>>,
    shallow: HashSet<gix::ObjectId>,
}

impl GixBackend {
    pub(crate) fn discover() -> Result<Self> {
        Self::discover_from(".")
    }

    pub(super) fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut repo = gix::discover_with_environment_overrides(directory)
            .map_err(|source| GitLsError::gix("discover repository", source))?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        let shallow = repo
            .shallow_commits()
            .map_err(|error| GitLsError::gix("read shallow boundary", error))?
            .map_or_else(HashSet::new, |ids| ids.iter().copied().collect());
        Ok(Self {
            repo,
            shallow,
            command: ProcessGit,
            parents: RefCell::default(),
            selection: RefCell::default(),
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
        let _perf = crate::diagnostics::operation("metadata");
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
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        let analysis = self.analyse_tips(main, tips)?;
        let result = analysis.unmerged.clone();
        *self.selection.borrow_mut() = Some(analysis);
        Ok(result)
    }

    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        let mut sorted = tips.to_vec();
        sorted.sort();
        sorted.dedup();
        if let Some(analysis) = self.selection.borrow().as_ref()
            && analysis.main == main
            && analysis.unmerged == sorted
        {
            return Ok(analysis.heads.clone());
        }
        Ok(self.analyse_tips(main, &sorted)?.heads)
    }

    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        self.distances(main, bases)
    }

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        let _perf = crate::diagnostics::operation("merge_base");
        let main_oid = Self::object_id(main_oid)?;
        let head_oid = Self::object_id(head_oid)?;
        let bases = self
            .repo
            .merge_bases_many(main_oid, &[head_oid])
            .map_err(|source| GitLsError::gix("find merge base", source))?;
        Ok(bases.first().map(ToString::to_string))
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        let _perf = crate::diagnostics::operation("ancestry_path");
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

        let mut nodes = Vec::new();
        for info in walk
            .all()
            .map_err(|source| GitLsError::gix("walk revisions", source))?
        {
            let info =
                info.map_err(|source| GitLsError::gix("read revision walk entry", source))?;
            let parents = self.effective_parents(info.id, info.parent_ids.into_vec());
            self.remember_parents(info.id, &parents);
            nodes.push((info.id, parents));
        }
        crate::diagnostics::count("ancestry_vertices", nodes.len());
        crate::diagnostics::count(
            "ancestry_edges",
            nodes.iter().map(|(_, parents)| parents.len()).sum(),
        );
        let included = base_oid.map(|base| super::graph::descendants(&nodes, base));
        let mut path: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| included.as_ref().is_none_or(|flags| flags[*index]))
            .map(|(_, (oid, _))| oid.to_string())
            .collect();
        crate::diagnostics::count("returned_commits", path.len());
        path.reverse();
        Ok(path)
    }
}

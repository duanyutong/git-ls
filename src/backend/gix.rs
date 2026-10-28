use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

mod diff;
mod graph;

use gix::bstr::ByteSlice as _;

use crate::error::{GitLsError, Result};
use crate::model::CommitMeta;

use super::metadata::{insert_commit_meta, missing_commit_aliases};
use super::process::ProcessGit;
use super::shell::{shell_query_branch_names, shell_query_revset};
use super::traits::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, DiffStatBackend,
    RepositoryStateBackend,
};

/// Repository backend that uses `gix` for native Git data and shells out only
/// for `git-branchless` revsets and Git's attribute-aware diff statistics.
pub(crate) struct GixBackend {
    repo: gix::Repository,
    command: ProcessGit,
    parents: RefCell<HashMap<gix::ObjectId, Vec<gix::ObjectId>>>,
    selection: RefCell<Option<graph::TipAnalysis>>,
    summaries: RefCell<Option<super::graph::LinearPaths<gix::ObjectId>>>,
    shallow: HashSet<gix::ObjectId>,
}

impl GixBackend {
    pub(crate) fn discover(command: ProcessGit) -> Result<Self> {
        let mut backend = Self::discover_from(".")?;
        backend.command = command;
        Ok(backend)
    }

    pub(crate) fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
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
            command: ProcessGit::default(),
            parents: RefCell::default(),
            selection: RefCell::default(),
            summaries: RefCell::default(),
        })
    }

    pub(crate) fn requires_shell_graph(&self) -> Result<bool> {
        if self.repo.common_dir().join("info/grafts").exists()
            || std::env::var_os("GIT_GRAFT_FILE").is_some()
            || std::env::var_os("GIT_REPLACE_REF_BASE").is_some()
        {
            return Ok(true);
        }
        Ok(self
            .repo
            .references()
            .map_err(|source| GitLsError::gix("open replacement references", source))?
            .prefixed("refs/replace/")
            .map_err(|source| GitLsError::gix("iterate replacement references", source))?
            .next()
            .transpose()
            .map_err(|source| GitLsError::gix("read replacement reference", source))?
            .is_some())
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

    fn query_rewrites(
        &self,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        super::shell::shell_query_rewrites(&self.command, oids, targets)
    }

    fn query_rewrites_in_path(
        &self,
        base: Option<&str>,
        head: &str,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        super::shell::shell_path_rewrites(&self.command, base, head, oids, targets)
    }
}

impl DiffStatBackend for GixBackend {
    fn diff_stats(&self, pairs: &[super::DiffPair]) -> Result<super::DiffBatch> {
        let mut trees = HashMap::new();
        for id in super::diff::commit_ids(pairs) {
            let commit = self
                .repo
                .find_commit(Self::object_id(id)?)
                .map_err(|error| GitLsError::gix("find diff endpoint", error))?;
            let tree = commit
                .tree_id()
                .map_err(|error| GitLsError::gix("read endpoint tree", error))?;
            trees.insert(id.to_string(), tree.detach().to_string());
        }
        super::diff::diff_with_trees(&self.command, pairs, &trees)
    }

    fn diff_context(&self, paths: &[String]) -> Option<u64> {
        super::GitCommand::diff_context(&self.command, paths)
    }

    fn diff_snapshot(&self, paths: &[String]) -> Option<super::DiffContext> {
        super::GitCommand::diff_snapshot(&self.command, paths)
    }

    fn diff_tree_pairs(
        &self,
        pairs: &[super::DiffPair],
        trees: &mut HashMap<String, String>,
    ) -> Result<Option<Vec<super::DiffPair>>> {
        for id in super::diff::commit_ids(pairs) {
            if trees.contains_key(id) {
                continue;
            }
            let commit = self
                .repo
                .find_commit(Self::object_id(id)?)
                .map_err(|error| GitLsError::gix("find diff endpoint", error))?;
            let tree = commit
                .tree_id()
                .map_err(|error| GitLsError::gix("read endpoint tree", error))?;
            trees.insert(id.to_string(), tree.detach().to_string());
        }
        let empty = gix::ObjectId::empty_tree(self.repo.object_hash()).to_string();
        super::diff::resolved_pairs(pairs, trees, &empty).map(Some)
    }

    fn diff_paths(&self, pairs: &[super::DiffPair]) -> Result<Vec<Vec<String>>> {
        pairs.iter().map(|pair| self.tree_paths(pair)).collect()
    }

    fn diff_tree_stats(&self, pairs: &[super::DiffPair]) -> Result<super::DiffBatch> {
        super::diff::tree_stats(&self.command, pairs)
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
    fn captured_shallow_boundary(&self) -> Option<Vec<String>> {
        let mut boundary: Vec<_> = self.shallow.iter().map(ToString::to_string).collect();
        boundary.sort_unstable();
        Some(boundary)
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for reference in self
            .repo
            .references()
            .map_err(|source| GitLsError::gix("open references", source))?
            .local_branches()
            .map_err(|source| GitLsError::gix("iterate local branches", source))?
        {
            let mut reference =
                reference.map_err(|source| GitLsError::gix("read local branch", source))?;
            // Following a symbolic ref mutates its target; retain its own name.
            let name = reference.name().shorten().to_str_lossy().into_owned();
            let id = reference
                .follow_to_object()
                .map_err(|source| GitLsError::gix("resolve local branch", source))?;
            result
                .entry(id.detach().to_string())
                .or_default()
                .push(name);
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
    fn prepare_ancestry(&self, main: &str, points: &[&str]) -> Result<()> {
        self.summaries.replace(None);
        let selection = self.selection.borrow();
        let Some(analysis) = selection.as_ref().filter(|analysis| analysis.main == main) else {
            return Ok(());
        };
        let Some(nodes) = &analysis.nodes else {
            return Ok(());
        };
        let points: HashSet<_> = points
            .iter()
            .map(|id| Self::object_id(id))
            .collect::<Result<_>>()?;
        crate::diagnostics::count("shared_ancestry_preparations", 1);
        if let Some(paths) = super::graph::linear_paths(nodes, &points) {
            crate::diagnostics::count("shared_ancestry_vertices", nodes.len());
            self.summaries.replace(Some(paths));
        }
        Ok(())
    }

    fn ancestry_summary(&self, main: &str, head: &str) -> Result<Option<super::AncestrySummary>> {
        if self
            .selection
            .borrow()
            .as_ref()
            .is_none_or(|selection| selection.main != main)
        {
            return Ok(None);
        }
        let summary = self
            .summaries
            .borrow()
            .as_ref()
            .and_then(|paths| paths.summary(&Self::object_id(head).ok()?));
        Ok(summary.map(|(base, points)| {
            crate::diagnostics::count("lane_summary_hits", 1);
            super::AncestrySummary {
                base: base.map(|id| id.to_string()),
                points: points
                    .into_iter()
                    .map(|(id, count)| (id.to_string(), count))
                    .collect(),
            }
        }))
    }

    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        let analysis = self.analyse_tips(main, tips)?;
        let result = analysis.unmerged.clone();
        *self.selection.borrow_mut() = Some(analysis);
        self.summaries.replace(None);
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
        if let Some(selection) = self.selection.borrow().as_ref()
            && selection.main == main_oid
            && let Some(base) = selection.bases.get(head_oid)
        {
            crate::diagnostics::count("merge_base_selection_hits", 1);
            return Ok(base.clone());
        }
        crate::diagnostics::count("merge_base_searches", 1);
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
        crate::diagnostics::maximum("peak_path_entries", path.len());
        crate::diagnostics::maximum("peak_parent_entries", self.parents.borrow().len());
        path.reverse();
        Ok(path)
    }
}

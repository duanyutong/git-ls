use std::collections::HashMap;

use super::diff::{DiffBatch, DiffContext, DiffPair};
use crate::error::Result;
use crate::model::CommitMeta;

pub(crate) struct AncestrySummary {
    pub(crate) base: Option<String>,
    pub(crate) points: Vec<(String, usize)>,
}

pub(crate) trait GitCommand {
    fn config_values(&self, keys: &[&str]) -> Result<HashMap<String, String>> {
        let mut values = HashMap::new();
        for key in keys {
            let value = self.run(&["config", "--get", key], true)?;
            if !value.trim().is_empty() {
                values.insert((*key).to_string(), value);
            }
        }
        Ok(values)
    }

    fn diff_context(&self, _paths: &[String]) -> Option<u64> {
        None
    }

    fn diff_snapshot(&self, paths: &[String]) -> Option<DiffContext> {
        self.diff_context(paths).map(DiffContext::uniform)
    }

    fn empty_tree(&self) -> Result<String> {
        self.run_input(&["hash-object", "-t", "tree", "--stdin"], "")
    }

    fn boolean_values(&self, values: &HashMap<String, String>) -> Result<HashMap<String, String>> {
        let mut result = HashMap::new();
        for key in ["git-ls.diffstat", "git-ls.cache", "git-ls.cachemaintenance"] {
            if values.contains_key(key) {
                let canonical = self.run(&["config", "--type=bool", "--get", key], false)?;
                result.insert(key.to_string(), canonical);
            }
        }
        Ok(result)
    }

    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String>;
    fn run_input(&self, args: &[&str], _input: &str) -> Result<String> {
        Err(crate::error::GitLsError::git_command(
            args.join(" "),
            "standard input is unsupported by this command adapter",
        ))
    }
}

pub(crate) trait BranchlessQueries {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;

    fn query_rewrites_in_path(
        &self,
        _base: Option<&str>,
        _head: &str,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        self.query_rewrites(oids, targets)
    }

    fn query_rewrites(
        &self,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut rewrites = HashMap::new();
        for oid in oids {
            let current = self.query_revset(&format!("current({oid})"), true)?;
            if let [replacement] = current.as_slice()
                && replacement != oid
                && targets.contains(replacement)
            {
                rewrites.insert(oid.clone(), replacement.clone());
            }
        }
        Ok(rewrites)
    }
}

pub(crate) trait CommitMetadataBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()>;
}

pub(crate) trait DiffStatBackend {
    /// Compare the layer's endpoint trees; an absent base denotes the empty tree.
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch>;
    fn diff_context(&self, paths: &[String]) -> Option<u64>;

    fn diff_snapshot(&self, paths: &[String]) -> Option<DiffContext> {
        self.diff_context(paths).map(DiffContext::uniform)
    }

    /// Resolve immutable endpoint trees, reusing already captured commit mappings.
    fn diff_tree_pairs(
        &self,
        _pairs: &[DiffPair],
        _trees: &mut HashMap<String, String>,
    ) -> Result<Option<Vec<DiffPair>>> {
        Ok(None)
    }

    /// Discover an attribute-independent superset of each comparison's paths.
    fn diff_paths(&self, _pairs: &[DiffPair]) -> Result<Vec<Vec<String>>> {
        Ok(Vec::new())
    }

    fn diff_tree_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        self.diff_stats(pairs)
    }
}

pub(crate) trait RepositoryStateBackend {
    /// Native adapters expose the boundary captured when opening their graph.
    /// Live command adapters do not retain a boundary between queries.
    fn captured_shallow_boundary(&self) -> Option<Vec<String>> {
        None
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>>;
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)>;
    fn main_branch_name(&self) -> Result<String>;

    fn upstream_branches(&self) -> Result<HashMap<String, crate::model::MainReference>> {
        Ok(HashMap::new())
    }
}

pub(crate) trait AncestryBackend {
    /// Prepare shared paths once for the complete branch-point selection.
    fn prepare_ancestry(&self, _main: &str, _points: &[&str]) -> Result<()> {
        Ok(())
    }

    fn ancestry_summary(&self, _main: &str, _head: &str) -> Result<Option<AncestrySummary>> {
        Ok(None)
    }

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>>;
    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>>;

    /// Select all supplied tips outside main in a collective query.
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>>;

    /// Compute independent heads collectively, without pairwise searches.
    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>>;

    /// Count all requested main-history ranges through shared analysis.
    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>>;
}

pub(crate) trait GitBackend:
    BranchlessQueries
    + CommitMetadataBackend
    + DiffStatBackend
    + RepositoryStateBackend
    + AncestryBackend
{
}

impl<T> GitBackend for T where
    T: BranchlessQueries
        + CommitMetadataBackend
        + DiffStatBackend
        + RepositoryStateBackend
        + AncestryBackend
        + ?Sized
{
}

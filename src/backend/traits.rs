use std::collections::HashMap;

use crate::error::Result;
use crate::model::CommitMeta;

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
}

pub(crate) trait CommitMetadataBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()>;
}

pub(crate) trait RepositoryStateBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>>;
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)>;
    fn main_branch_name(&self) -> Result<String>;
}

pub(crate) trait AncestryBackend {
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
    BranchlessQueries + CommitMetadataBackend + RepositoryStateBackend + AncestryBackend
{
}

impl<T> GitBackend for T where
    T: BranchlessQueries
        + CommitMetadataBackend
        + RepositoryStateBackend
        + AncestryBackend
        + ?Sized
{
}

use std::collections::HashMap;

use crate::error::Result;
use crate::model::CommitMeta;

pub(crate) trait GitCommand {
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

    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        tips.iter()
            .filter(|tip| tip.as_str() != main)
            .filter_map(|tip| match self.merge_base(tip, main) {
                Ok(base) if base.as_ref() == Some(tip) => None,
                Ok(_) => Some(Ok(tip.clone())),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn independent_heads(&self, _main: &str, tips: &[String]) -> Result<Vec<String>> {
        let mut heads = Vec::new();
        for tip in tips {
            let mut intermediate = false;
            for other in tips.iter().filter(|other| *other != tip) {
                if self.merge_base(tip, other)?.as_ref() == Some(tip) {
                    intermediate = true;
                    break;
                }
            }
            if !intermediate {
                heads.push(tip.clone());
            }
        }
        Ok(heads)
    }

    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        bases
            .iter()
            .map(|base| {
                let distance = if base == main {
                    Some(0)
                } else {
                    let length = self.ancestry_path(Some(base), main)?.len();
                    (length > 0).then_some(length)
                };
                Ok((base.clone(), distance))
            })
            .collect()
    }
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

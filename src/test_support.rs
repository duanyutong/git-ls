use std::cell::RefCell;
use std::collections::HashMap;

use crate::backend::GitCommand;
use crate::error::{GitLsError, Result};

pub(crate) const TEST_NOW: i64 = 1_700_000_120;
pub(crate) const TEST_COMMIT_TIME: i64 = 1_700_000_000;

#[derive(Default)]
pub(crate) struct MockGit {
    responses: HashMap<Vec<String>, String>,
    calls: RefCell<Vec<Vec<String>>>,
    failures: std::collections::HashSet<Vec<String>>,
}

impl MockGit {
    pub(crate) fn with_error(mut self, args: &[&str]) -> Self {
        self.failures
            .insert(args.iter().map(|arg| (*arg).to_string()).collect());
        self
    }

    pub(crate) fn with(mut self, args: &[&str], output: &str) -> Self {
        self.responses.insert(
            args.iter().map(|arg| (*arg).to_string()).collect(),
            output.to_string(),
        );
        self
    }

    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}

impl GitCommand for MockGit {
    fn run_input(&self, args: &[&str], _input: &str) -> Result<String> {
        self.run(args, false)
    }

    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        self.calls.borrow_mut().push(key.clone());
        if self.failures.contains(&key) {
            return Err(GitLsError::TestFixture("forced Git failure".to_string()));
        }
        if let Some(output) = self.responses.get(&key) {
            return Ok(output.clone());
        }
        if allow_failure {
            return Ok(String::new());
        }
        Err(GitLsError::TestFixture(format!(
            "missing mock git response: {}",
            args.join(" ")
        )))
    }
}

pub(crate) struct OmitMetadata<'a> {
    pub(crate) inner: &'a dyn crate::backend::GitBackend,
    pub(crate) omitted: &'a str,
}

impl crate::backend::BranchlessQueries for OmitMetadata<'_> {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.inner.query_revset(revset, hidden)
    }
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.inner.query_branch_names(revset, hidden)
    }
}
impl crate::backend::RepositoryStateBackend for OmitMetadata<'_> {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        self.inner.local_branches_by_oid()
    }
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        self.inner.current_head_and_branch()
    }
    fn main_branch_name(&self) -> Result<String> {
        self.inner.main_branch_name()
    }
}
impl crate::backend::AncestryBackend for OmitMetadata<'_> {
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.inner.unmerged_tips(main, tips)
    }

    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.inner.independent_heads(main, tips)
    }
    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        self.inner.main_distances(main, bases)
    }

    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        self.inner.merge_base(main, head)
    }
    fn ancestry_path(&self, base: Option<&str>, head: &str) -> Result<Vec<String>> {
        self.inner.ancestry_path(base, head)
    }
}
impl crate::backend::CommitMetadataBackend for OmitMetadata<'_> {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, crate::model::CommitMeta>,
    ) -> Result<()> {
        let included: Vec<_> = oids
            .iter()
            .copied()
            .filter(|oid| *oid != self.omitted)
            .collect();
        self.inner.cache_commit_metas(&included, cache)
    }
}

pub(crate) fn reference_unmerged(
    git: &dyn crate::backend::AncestryBackend,
    main: &str,
    tips: &[String],
) -> Result<Vec<String>> {
    tips.iter()
        .filter(|tip| tip.as_str() != main)
        .filter_map(|tip| match git.merge_base(tip, main) {
            Ok(base) if base.as_ref() == Some(tip) => None,
            Ok(_) => Some(Ok(tip.clone())),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub(crate) fn reference_heads(
    git: &dyn crate::backend::AncestryBackend,
    _main: &str,
    tips: &[String],
) -> Result<Vec<String>> {
    let mut heads = Vec::new();
    for tip in tips {
        let mut intermediate = false;
        for other in tips.iter().filter(|other| *other != tip) {
            if git.merge_base(tip, other)?.as_ref() == Some(tip) {
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

pub(crate) fn reference_distances(
    git: &dyn crate::backend::AncestryBackend,
    main: &str,
    bases: &[String],
) -> Result<HashMap<String, Option<usize>>> {
    bases
        .iter()
        .map(|base| {
            let distance = if base == main {
                Some(0)
            } else {
                let length = git.ancestry_path(Some(base), main)?.len();
                (length > 0).then_some(length)
            };
            Ok((base.clone(), distance))
        })
        .collect()
}

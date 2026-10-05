use std::cell::RefCell;
use std::collections::HashMap;

use super::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, GitBackend, RepositoryStateBackend,
};
use crate::error::Result;
use crate::model::CommitMeta;

const QUERY_LIMIT: usize = 65_536;

/// Reuses completed, directional queries within one invocation. Branchless state
/// is deliberately never persisted: local refs do not identify its event log.
pub(crate) struct Session<'a> {
    inner: &'a dyn GitBackend,
    rewrites: RefCell<HashMap<String, Vec<String>>>,
    bases: RefCell<HashMap<(String, String), Option<String>>>,
}

impl<'a> Session<'a> {
    pub(crate) fn new(inner: &'a dyn GitBackend) -> Self {
        Self {
            inner,
            rewrites: RefCell::default(),
            bases: RefCell::default(),
        }
    }
}

impl BranchlessQueries for Session<'_> {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        if !hidden || !revset.starts_with("current(") {
            return self.inner.query_revset(revset, hidden);
        }
        if let Some(result) = self.rewrites.borrow().get(revset) {
            crate::diagnostics::count("rewrite_cache_hits", 1);
            return Ok(result.clone());
        }
        let result = self.inner.query_revset(revset, hidden)?;
        if self.rewrites.borrow().len() < QUERY_LIMIT {
            self.rewrites
                .borrow_mut()
                .insert(revset.to_string(), result.clone());
        }
        Ok(result)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.inner.query_branch_names(revset, hidden)
    }
}

impl CommitMetadataBackend for Session<'_> {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        self.inner.cache_commit_metas(oids, cache)
    }
}

impl RepositoryStateBackend for Session<'_> {
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

impl AncestryBackend for Session<'_> {
    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        let key = (main.to_string(), head.to_string());
        if let Some(result) = self.bases.borrow().get(&key) {
            crate::diagnostics::count("merge_base_cache_hits", 1);
            return Ok(result.clone());
        }
        let result = self.inner.merge_base(main, head)?;
        if self.bases.borrow().len() < QUERY_LIMIT {
            self.bases.borrow_mut().insert(key, result.clone());
        }
        Ok(result)
    }
    fn ancestry_path(&self, base: Option<&str>, head: &str) -> Result<Vec<String>> {
        self.inner.ancestry_path(base, head)
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockGit;

    #[test]
    fn memoises_unchanged_empty_and_ambiguous_rewrites() {
        for result in ["oid", "", "a\nb"] {
            let git = MockGit::default().with(
                &["branchless", "query", "-r", "--hidden", "current(oid)"],
                result,
            );
            let session = Session::new(&git);
            let first = session.query_revset("current(oid)", true).unwrap();
            assert_eq!(first, session.query_revset("current(oid)", true).unwrap());
            assert_eq!(git.calls().len(), 1);
        }
    }
}

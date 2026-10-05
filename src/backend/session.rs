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
    facts: RefCell<Facts>,
    inputs: Option<Inputs>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Facts {
    pub(crate) bases: HashMap<String, Option<String>>,
    pub(crate) distances: HashMap<String, Option<usize>>,
    pub(crate) metadata: HashMap<String, CommitMeta>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct Inputs {
    pub(crate) branches: std::collections::BTreeMap<String, Vec<String>>,
    pub(crate) configured_main: String,
}

impl Inputs {
    pub(crate) fn capture(git: &dyn GitBackend) -> Result<Self> {
        let mut branches: std::collections::BTreeMap<_, _> =
            git.local_branches_by_oid()?.into_iter().collect();
        for names in branches.values_mut() {
            names.sort();
        }
        Ok(Self {
            branches,
            configured_main: git.main_branch_name()?,
        })
    }
}

impl<'a> Session<'a> {
    pub(crate) fn with_facts(inner: &'a dyn GitBackend, inputs: Inputs, facts: Facts) -> Self {
        Self {
            inner,
            inputs: Some(inputs),
            facts: RefCell::new(facts),
            rewrites: RefCell::default(),
        }
    }
    pub(crate) fn facts(&self) -> Facts {
        self.facts.borrow().clone()
    }

    pub(crate) fn new(inner: &'a dyn GitBackend) -> Self {
        Self {
            inner,
            rewrites: RefCell::default(),
            facts: RefCell::default(),
            inputs: None,
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
        {
            let facts = self.facts.borrow();
            for oid in oids {
                if !cache.contains_key(*oid)
                    && let Some(meta) = facts.metadata.get(*oid)
                {
                    cache.insert((*oid).to_string(), meta.clone());
                    crate::diagnostics::count("persistent_metadata_hits", 1);
                }
            }
        }
        self.inner.cache_commit_metas(oids, cache)?;
        let mut facts = self.facts.borrow_mut();
        for oid in oids {
            if facts.metadata.len() < QUERY_LIMIT
                && let Some(meta) = cache.get(*oid)
            {
                facts.metadata.insert((*oid).to_string(), meta.clone());
            }
        }
        Ok(())
    }
}

impl RepositoryStateBackend for Session<'_> {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        if let Some(inputs) = &self.inputs {
            return Ok(inputs.branches.clone().into_iter().collect());
        }
        self.inner.local_branches_by_oid()
    }
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        self.inner.current_head_and_branch()
    }
    fn main_branch_name(&self) -> Result<String> {
        if let Some(inputs) = &self.inputs {
            return Ok(inputs.configured_main.clone());
        }
        self.inner.main_branch_name()
    }
}

impl AncestryBackend for Session<'_> {
    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        let key = format!("{main}:{head}");
        if let Some(result) = self.facts.borrow().bases.get(&key) {
            crate::diagnostics::count("merge_base_cache_hits", 1);
            return Ok(result.clone());
        }
        let result = self.inner.merge_base(main, head)?;
        if self.facts.borrow().bases.len() < QUERY_LIMIT {
            self.facts.borrow_mut().bases.insert(key, result.clone());
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
        let mut result = HashMap::new();
        let mut missing = Vec::new();
        {
            let facts = self.facts.borrow();
            for base in bases {
                if let Some(distance) = facts.distances.get(&format!("{main}:{base}")) {
                    result.insert(base.clone(), *distance);
                    crate::diagnostics::count("distance_cache_hits", 1);
                } else {
                    missing.push(base.clone());
                }
            }
        }
        if !missing.is_empty() {
            let computed = self.inner.main_distances(main, &missing)?;
            let mut facts = self.facts.borrow_mut();
            for (base, distance) in &computed {
                if facts.distances.len() < QUERY_LIMIT {
                    facts.distances.insert(format!("{main}:{base}"), *distance);
                }
            }
            result.extend(computed);
        }
        Ok(result)
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

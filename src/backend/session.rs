use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, GitBackend, RepositoryStateBackend,
};
use crate::cache::Facts;
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
    limit: usize,
    new_facts: Cell<bool>,
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
            limit: QUERY_LIMIT,
            new_facts: Cell::new(false),
            facts: RefCell::new(facts),
            rewrites: RefCell::default(),
        }
    }
    pub(crate) const fn has_new_facts(&self) -> bool {
        self.new_facts.get()
    }

    pub(crate) fn into_facts(self) -> Facts {
        self.facts.into_inner()
    }

    pub(crate) fn new(inner: &'a dyn GitBackend) -> Self {
        Self {
            inner,
            rewrites: RefCell::default(),
            facts: RefCell::default(),
            inputs: None,
            limit: QUERY_LIMIT,
            new_facts: Cell::new(false),
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
        if self.rewrites.borrow().len() < self.limit {
            self.rewrites
                .borrow_mut()
                .insert(revset.to_string(), result.clone());
        }
        Ok(result)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.inner.query_branch_names(revset, hidden)
    }

    fn query_rewrites(
        &self,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        self.rewrite_results(oids, targets, |missing| {
            self.inner.query_rewrites(missing, targets)
        })
    }

    fn query_rewrites_in_path(
        &self,
        base: Option<&str>,
        head: &str,
        oids: &[String],
        targets: &[String],
    ) -> Result<HashMap<String, String>> {
        self.rewrite_results(oids, targets, |missing| {
            self.inner
                .query_rewrites_in_path(base, head, missing, targets)
        })
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
            if self.inputs.is_some() && facts.metadata.contains_key(*oid) {
                facts.touch(oid);
            }
            if facts.metadata.len() < self.limit
                && let Some(meta) = cache.get(*oid)
            {
                if !facts.metadata.contains_key(*oid) {
                    self.new_facts.set(true);
                }
                facts.metadata.insert((*oid).to_string(), meta.clone());
                if self.inputs.is_some() {
                    facts.touch(oid);
                }
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

mod statistics;

impl AncestryBackend for Session<'_> {
    fn prepare_ancestry(&self, main: &str, points: &[&str]) -> Result<()> {
        self.inner.prepare_ancestry(main, points)
    }

    fn ancestry_summary(&self, main: &str, head: &str) -> Result<Option<super::AncestrySummary>> {
        self.inner.ancestry_summary(main, head)
    }

    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        let key = format!("{main}:{head}");
        let cached = self.facts.borrow().bases.get(&key).cloned();
        if let Some(result) = cached {
            if self.inputs.is_some() {
                self.facts.borrow_mut().touch(&key);
            }
            crate::diagnostics::count("merge_base_cache_hits", 1);
            return Ok(result);
        }
        let result = self.inner.merge_base(main, head)?;
        if self.facts.borrow().bases.len() < self.limit {
            let mut facts = self.facts.borrow_mut();
            if self.inputs.is_some() {
                facts.touch(&key);
            }
            facts.bases.insert(key, result.clone());
            self.new_facts.set(true);
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
            let mut facts = self.facts.borrow_mut();
            for base in bases {
                if let Some(distance) = facts.distances.get(&format!("{main}:{base}")) {
                    result.insert(base.clone(), *distance);
                    if self.inputs.is_some() {
                        facts.touch(&format!("{main}:{base}"));
                    }
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
                if facts.distances.len() < self.limit {
                    let key = format!("{main}:{base}");
                    if self.inputs.is_some() {
                        facts.touch(&key);
                    }
                    facts.distances.insert(key, *distance);
                    self.new_facts.set(true);
                }
            }
            result.extend(computed);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/session_diff.rs"]
mod diff_tests;

#[cfg(test)]
#[path = "tests/session_reuse_tests.rs"]
mod reuse_tests;

impl Session<'_> {
    fn rewrite_results(
        &self,
        oids: &[String],
        targets: &[String],
        read: impl FnOnce(&[String]) -> Result<HashMap<String, String>>,
    ) -> Result<HashMap<String, String>> {
        let mut ordered = targets.to_vec();
        ordered.sort_unstable();
        let target_key = crate::cache::hash(&ordered);
        let key = |oid: &str| format!("rewrites:{target_key}:{oid}");
        let missing: Vec<_> = oids
            .iter()
            .filter(|oid| !self.rewrites.borrow().contains_key(&key(oid)))
            .cloned()
            .collect();
        let mut result = if missing.is_empty() {
            HashMap::new()
        } else {
            read(&missing)?
        };
        let mut retained = self.rewrites.borrow_mut();
        for oid in oids {
            if let Some(value) = retained.get(&key(oid)) {
                if let Some(replacement) = value.first() {
                    result.insert(oid.clone(), replacement.clone());
                }
                crate::diagnostics::count("rewrite_cache_hits", 1);
            } else if retained.len() < self.limit {
                retained.insert(key(oid), result.get(oid).cloned().into_iter().collect());
            }
        }
        Ok(result)
    }
}

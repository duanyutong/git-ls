use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, DiffBatch, DiffPair,
    DiffStatBackend, GitBackend, RepositoryStateBackend,
};
use crate::cache::{Facts, facts::DiffFacts};
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

impl Session<'_> {
    fn discard_diff_stats(&self) {
        let previous = std::mem::take(&mut self.facts.borrow_mut().diff);
        if previous != DiffFacts::default() {
            self.new_facts.set(true);
        }
    }

    fn checked_diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        let batch = self.inner.diff_stats(pairs)?;
        if batch.stats.len() != pairs.len() {
            return Err(crate::error::GitLsError::git_command(
                "diff statistics",
                "backend returned an incorrect result count",
            ));
        }
        Ok(batch)
    }

    fn record_diff_stats(
        &self,
        pairs: &[DiffPair],
        batch: &mut DiffBatch,
        previous: &DiffFacts,
        valid: bool,
    ) -> Result<bool> {
        let mut paths = if valid {
            previous.paths.clone()
        } else {
            Vec::new()
        };
        paths.extend(batch.paths.iter().cloned());
        paths.sort_unstable();
        paths.dedup();
        // Discover dependencies first, then measure under a captured context.
        // Mutable attributes cannot be validated solely from endpoint IDs.
        let Some(before) = self.inner.diff_context(&paths) else {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=unsupported_context"));
            return Ok(false);
        };
        *batch = self.checked_diff_stats(pairs)?;
        let after = self.inner.diff_context(&paths);
        let dependencies_covered = batch
            .paths
            .iter()
            .all(|path| paths.binary_search(path).is_ok());
        if after != Some(before)
            || !dependencies_covered
            || (valid && self.inner.diff_context(&previous.paths) != previous.context)
        {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=context_changed"));
            return Ok(false);
        }
        let mut facts = self.facts.borrow_mut();
        facts.diff.context = after;
        facts.diff.paths = paths;
        for (pair, stat) in pairs.iter().zip(&batch.stats) {
            if facts.diff.values.len() < self.limit {
                let key = pair.key();
                facts.diff.values.insert(key.clone(), *stat);
                facts.touch(&key);
            }
        }
        self.new_facts.set(true);
        Ok(true)
    }
}

impl DiffStatBackend for Session<'_> {
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        if self.inputs.is_none() || pairs.is_empty() {
            return self.inner.diff_stats(pairs);
        }
        let previous = self.facts.borrow().diff.clone();
        let valid = previous.context.is_some()
            && self.inner.diff_context(&previous.paths) == previous.context;
        if !valid {
            self.discard_diff_stats();
        }
        let missing: Vec<_> = pairs
            .iter()
            .filter(|pair| !valid || !previous.values.contains_key(&pair.key()))
            .cloned()
            .collect();
        crate::diagnostics::count("diff_cache_hits", pairs.len() - missing.len());
        crate::diagnostics::count("diff_cache_misses", missing.len());
        if missing.is_empty() {
            return Ok(DiffBatch {
                stats: pairs
                    .iter()
                    .filter_map(|pair| previous.values.get(&pair.key()).copied())
                    .collect(),
                paths: previous.paths,
            });
        }
        let mut batch = self.checked_diff_stats(&missing)?;
        let stable = self.record_diff_stats(&missing, &mut batch, &previous, valid)?;
        if valid && !stable {
            self.discard_diff_stats();
            return self.checked_diff_stats(pairs);
        }
        let mut live = if valid {
            previous.values
        } else {
            HashMap::new()
        };
        live.extend(
            missing
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat)),
        );
        Ok(DiffBatch {
            stats: pairs
                .iter()
                .filter_map(|pair| live.get(&pair.key()).copied())
                .collect(),
            paths: batch.paths,
        })
    }

    fn diff_context(&self, paths: &[String]) -> Option<u64> {
        self.inner.diff_context(paths)
    }
}

impl AncestryBackend for Session<'_> {
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

//! Tree-keyed statistics and path-specific validity, independent of command execution.

use std::collections::{BTreeSet, HashMap};

use super::Session;
use crate::backend::{DiffBatch, DiffContext, DiffPair, DiffStatBackend};
use crate::cache::facts::DiffValue;
use crate::error::{GitLsError, Result};
use crate::model::DiffStat;

fn invalid() -> GitLsError {
    GitLsError::git_command(
        "diff statistics",
        "backend returned an incorrect result count",
    )
}

fn assembled(
    pairs: &[DiffPair],
    values: &HashMap<String, DiffStat>,
    paths: Vec<String>,
) -> Result<DiffBatch> {
    Ok(DiffBatch {
        stats: pairs
            .iter()
            .map(|pair| {
                if pair.base.as_ref() == Some(&pair.head) {
                    Ok(DiffStat::default())
                } else {
                    values.get(&pair.key()).copied().ok_or_else(invalid)
                }
            })
            .collect::<Result<_>>()?,
        paths,
    })
}

impl Session<'_> {
    fn checked_tree_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        let batch = self.inner.diff_tree_stats(pairs)?;
        if batch.stats.len() != pairs.len() {
            return Err(invalid());
        }
        Ok(batch)
    }

    fn statistics_paths(&self, pairs: &[DiffPair]) -> Result<HashMap<String, Vec<String>>> {
        let mut paths = HashMap::new();
        let mut unknown = Vec::new();
        {
            let facts = self.facts.borrow();
            for pair in pairs {
                let key = pair.key();
                let retained = facts
                    .diff
                    .scopes
                    .values()
                    .find_map(|scope| scope.values.get(&key));
                if let Some(value) = retained {
                    paths.insert(key, value.paths.clone());
                } else {
                    unknown.push(pair.clone());
                }
            }
        }
        if !unknown.is_empty() {
            let discovered = self.inner.diff_paths(&unknown)?;
            if discovered.len() != unknown.len() {
                return Err(invalid());
            }
            paths.extend(unknown.iter().zip(discovered).map(|(pair, mut paths)| {
                paths.sort_unstable();
                paths.dedup();
                (pair.key(), paths)
            }));
        }
        Ok(paths)
    }

    fn cached_statistics(
        &self,
        pairs: &[DiffPair],
        paths: &HashMap<String, Vec<String>>,
        context: &DiffContext,
    ) -> HashMap<String, DiffStat> {
        let mut facts = self.facts.borrow_mut();
        let used = facts.diff.tick();
        let Some(scope) = facts.diff.scopes.get_mut(&context.scope) else {
            if !facts.diff.scopes.is_empty() {
                crate::diagnostics::count("diff_context_switches", 1);
            }
            return HashMap::new();
        };
        scope.used = used;
        let mut values = HashMap::new();
        for pair in pairs {
            let key = pair.key();
            if let Some(value) = scope.values.get_mut(&key)
                && paths
                    .get(&key)
                    .is_some_and(|paths| value.fingerprint == context.fingerprint(paths))
            {
                value.used = used;
                values.insert(key, value.stat);
            }
        }
        values
    }

    fn retain_statistics(
        &self,
        pairs: &[DiffPair],
        batch: &DiffBatch,
        paths: &HashMap<String, Vec<String>>,
        context: &DiffContext,
    ) {
        let mut facts = self.facts.borrow_mut();
        let used = facts.diff.tick();
        let scope = facts.diff.scopes.entry(context.scope).or_default();
        scope.used = used;
        for (pair, stat) in pairs.iter().zip(&batch.stats) {
            if scope.values.len() < self.limit
                && let Some(paths) = paths.get(&pair.key())
            {
                scope.values.insert(
                    pair.key(),
                    DiffValue {
                        stat: *stat,
                        fingerprint: context.fingerprint(paths),
                        paths: paths.clone(),
                        used,
                    },
                );
            }
        }
        self.new_facts.set(true);
    }

    fn reuse_statistics(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        let mut unique = HashMap::new();
        for pair in pairs
            .iter()
            .filter(|pair| pair.base.as_ref() != Some(&pair.head))
        {
            unique.entry(pair.key()).or_insert_with(|| pair.clone());
        }
        let mut requested: Vec<_> = unique.into_values().collect();
        requested.sort_unstable_by_key(DiffPair::key);
        if requested.is_empty() {
            crate::diagnostics::count("diff_equal_tree_skips", pairs.len());
            return assembled(pairs, &HashMap::new(), Vec::new());
        }
        let dependencies = self.statistics_paths(&requested)?;
        let paths: Vec<_> = dependencies
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        crate::diagnostics::count("diff_dependency_paths", paths.len());
        let dependency_bytes: usize = paths.iter().map(String::len).sum();
        if dependency_bytes as u64 > crate::cache::ENTRY_LIMIT {
            crate::diagnostics::count("diff_admission_bypasses", 1);
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=dependency_limit"));
            let batch = self.checked_tree_stats(&requested)?;
            let values = requested
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat))
                .collect();
            return assembled(pairs, &values, batch.paths);
        }
        let Some(before) = self.inner.diff_snapshot(&paths) else {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=unsupported_context"));
            let batch = self.checked_tree_stats(&requested)?;
            let values = requested
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat))
                .collect();
            return assembled(pairs, &values, batch.paths);
        };
        let mut values = self.cached_statistics(&requested, &dependencies, &before);
        let missing: Vec<_> = requested
            .iter()
            .filter(|pair| !values.contains_key(&pair.key()))
            .cloned()
            .collect();
        crate::diagnostics::count("diff_cache_hits", pairs.len() - missing.len());
        crate::diagnostics::count("diff_tree_cache_hits", requested.len() - missing.len());
        crate::diagnostics::count("diff_cache_misses", missing.len());
        if missing.is_empty() {
            return assembled(pairs, &values, paths);
        }
        let batch = self.checked_tree_stats(&missing)?;
        let covered = batch
            .paths
            .iter()
            .all(|path| paths.binary_search(path).is_ok());
        let stable = covered && self.inner.diff_snapshot(&paths).as_ref() == Some(&before);
        if stable {
            self.retain_statistics(&missing, &batch, &dependencies, &before);
        } else if !values.is_empty() {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=context_changed"));
            let batch = self.checked_tree_stats(&requested)?;
            let values = requested
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat))
                .collect();
            return assembled(pairs, &values, batch.paths);
        }
        values.extend(
            missing
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat)),
        );
        assembled(pairs, &values, batch.paths)
    }
}

impl DiffStatBackend for Session<'_> {
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        if self.inputs.is_none() || pairs.is_empty() {
            return self.inner.diff_stats(pairs);
        }
        let trees = {
            let mut facts = self.facts.borrow_mut();
            let before = facts.diff.trees.len();
            let result = self.inner.diff_tree_pairs(pairs, &mut facts.diff.trees)?;
            if facts.diff.trees.len() != before {
                self.new_facts.set(true);
            }
            for id in super::super::diff::commit_ids(pairs) {
                facts.touch(id);
            }
            result
        };
        let Some(trees) = trees else {
            return self.inner.diff_stats(pairs);
        };
        if trees.len() != pairs.len() {
            return Err(invalid());
        }
        self.reuse_statistics(&trees)
    }

    fn diff_context(&self, paths: &[String]) -> Option<u64> {
        self.inner.diff_context(paths)
    }
}

#[cfg(test)]
#[path = "../tests/statistics_policy.rs"]
mod tests;

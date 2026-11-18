//! Tree-keyed statistics and path-specific validity, independent of command execution.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::Session;
use crate::backend::{DiffBatch, DiffContext, DiffPair, DiffStatBackend};
use crate::cache::facts::DiffValue;
use crate::error::{GitLsError, Result};
use crate::model::DiffStat;

type Dependencies = HashMap<String, Rc<Vec<String>>>;

mod history;

#[derive(Clone)]
pub(super) struct Transition {
    previous: DiffPair,
    paths: Rc<Vec<String>>,
}

struct StatisticsSelection<'a> {
    dependencies: &'a Dependencies,
    pairs: Vec<DiffPair>,
    paths: Vec<String>,
    all_paths: BTreeSet<&'a str>,
}

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

    fn live_statistics(&self, pairs: &[DiffPair], requested: &[DiffPair]) -> Result<DiffBatch> {
        let batch = self.checked_tree_stats(requested)?;
        let values = requested
            .iter()
            .zip(batch.stats)
            .map(|(pair, stat)| (pair.key(), stat))
            .collect();
        assembled(pairs, &values, batch.paths)
    }

    fn statistics_paths(&self, pairs: &[DiffPair]) -> Result<Dependencies> {
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
                    .filter_map(|scope| scope.values.get(&key))
                    .collect::<Vec<_>>();
                if let Some(first) = retained.first() {
                    let depth = retained
                        .iter()
                        .map(|value| value.dependency_depth)
                        .max()
                        .unwrap_or(0);
                    self.dependency_depths
                        .borrow_mut()
                        .insert(key.clone(), depth);
                    let dependencies = if retained.iter().all(|value| value.paths == first.paths) {
                        first.paths.clone()
                    } else {
                        Rc::new(
                            retained
                                .iter()
                                .flat_map(|value| value.paths.iter())
                                .cloned()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                        )
                    };
                    paths.insert(key, dependencies);
                } else {
                    unknown.push(pair.clone());
                }
            }
        }
        if !unknown.is_empty() {
            paths.extend(self.discover_statistics_paths(&unknown)?);
        }
        Ok(paths)
    }

    fn cached_statistics(
        &self,
        pairs: &[DiffPair],
        paths: &Dependencies,
        context: &DiffContext,
    ) -> HashMap<String, DiffStat> {
        let mut facts = self.facts.borrow_mut();
        let used = facts.diff.tick();
        let Some(scope) = facts.diff.scopes.get_mut(&context.scope) else {
            if !facts.diff.scopes.is_empty() {
                crate::diagnostics::count("diff_context_switches", 1);
            }
            for pair in pairs {
                crate::diagnostics::event(format_args!(
                    "diff_cache pair={} context={} status=miss reason=context_absent",
                    pair.key(),
                    context.scope
                ));
            }
            return HashMap::new();
        };
        scope.used = used;
        let mut values = HashMap::new();
        for pair in pairs {
            let key = pair.key();
            let reason = match scope.values.get_mut(&key) {
                None => "pair_absent",
                Some(value) if value.configuration != context.configuration => {
                    "configuration_changed"
                }
                Some(_) if !paths.contains_key(&key) => "dependencies_absent",
                Some(value) if value.fingerprint == context.fingerprint(&value.paths) => {
                    value.used = used;
                    values.insert(key.clone(), value.stat);
                    "valid"
                }
                Some(_) => "attributes_changed",
            };
            crate::diagnostics::event(format_args!(
                "diff_cache pair={key} context={} status={} reason={reason}",
                context.scope,
                if values.contains_key(&key) {
                    "hit"
                } else {
                    "miss"
                }
            ));
        }
        values
    }

    fn retain_statistics(
        &self,
        pairs: &[DiffPair],
        batch: &DiffBatch,
        paths: &Dependencies,
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
                        configuration: context.configuration,
                        dependency_depth: self
                            .dependency_depths
                            .borrow()
                            .get(&pair.key())
                            .copied()
                            .unwrap_or(0),
                        paths: paths.clone(),
                        used,
                    },
                );
                crate::diagnostics::event(format_args!(
                    "diff_admission pair={} context={} status=retained paths={}",
                    pair.key(),
                    context.scope,
                    paths.len()
                ));
            } else {
                crate::diagnostics::event(format_args!(
                    "diff_admission pair={} context={} status=rejected reason={}",
                    pair.key(),
                    context.scope,
                    if scope.values.len() >= self.limit {
                        "entry_count"
                    } else {
                        "missing_dependencies"
                    }
                ));
            }
        }
        self.new_facts.set(true);
    }

    fn statistics_selection<'a>(
        &self,
        requested: &[DiffPair],
        dependencies: &'a Dependencies,
    ) -> StatisticsSelection<'a> {
        let all_paths: BTreeSet<_> = dependencies
            .values()
            .flat_map(|paths| paths.iter().map(String::as_str))
            .collect();
        crate::diagnostics::count("diff_dependency_paths", all_paths.len());
        let bytes = all_paths
            .iter()
            .map(|path| path.len().saturating_add(1))
            .fold(0usize, usize::saturating_add);
        if bytes as u64 <= crate::cache::ENTRY_LIMIT {
            return StatisticsSelection {
                dependencies,
                pairs: requested.to_vec(),
                paths: all_paths.iter().map(|path| (*path).to_owned()).collect(),
                all_paths,
            };
        }
        let facts = self.facts.borrow();
        let mut ranked: Vec<_> = requested
            .iter()
            .filter_map(|pair| dependencies.get(&pair.key()).map(|paths| (pair, paths)))
            .collect();
        ranked.sort_by_cached_key(|(pair, paths)| {
            let key = pair.key();
            let cached = facts
                .diff
                .scopes
                .values()
                .any(|scope| scope.values.contains_key(&key));
            (
                std::cmp::Reverse(cached),
                std::cmp::Reverse(paths.len()),
                key,
            )
        });
        let mut selected: BTreeSet<&str> = BTreeSet::new();
        let mut selected_pairs = Vec::new();
        let mut selected_bytes = 0usize;
        for (pair, paths) in ranked {
            let additional = paths
                .iter()
                .filter(|path| !selected.contains(path.as_str()))
                .map(|path| path.len().saturating_add(1))
                .fold(0usize, usize::saturating_add);
            if selected_bytes.saturating_add(additional) as u64 > crate::cache::ENTRY_LIMIT {
                crate::diagnostics::count("diff_admission_bypasses", 1);
                crate::diagnostics::event(format_args!(
                    "diff_cache pair={} status=bypass reason=dependency_budget",
                    pair.key()
                ));
                continue;
            }
            selected_bytes = selected_bytes.saturating_add(additional);
            selected.extend(paths.iter().map(String::as_str));
            selected_pairs.push(pair.clone());
        }
        StatisticsSelection {
            dependencies,
            pairs: selected_pairs,
            paths: selected.into_iter().map(str::to_owned).collect(),
            all_paths,
        }
    }

    fn selected_statistics(
        &self,
        pairs: &[DiffPair],
        requested: &[DiffPair],
        selection: &StatisticsSelection<'_>,
        before: &DiffContext,
    ) -> Result<DiffBatch> {
        self.facts.borrow_mut().diff.active.extend(
            selection
                .pairs
                .iter()
                .map(|pair| (before.scope, pair.key())),
        );
        let mut values = self.cached_statistics(&selection.pairs, selection.dependencies, before);
        let missing: Vec<_> = requested
            .iter()
            .filter(|pair| !values.contains_key(&pair.key()))
            .cloned()
            .collect();
        crate::diagnostics::count("diff_cache_hits", pairs.len() - missing.len());
        crate::diagnostics::count("diff_tree_cache_hits", requested.len() - missing.len());
        crate::diagnostics::count("diff_cache_misses", missing.len());
        if missing.is_empty() {
            return assembled(pairs, &values, selection.paths.clone());
        }
        let batch = self.checked_tree_stats(&missing)?;
        let covered = batch
            .paths
            .iter()
            .all(|path| selection.all_paths.contains(path.as_str()));
        let stable = covered && self.inner.diff_snapshot(&selection.paths).as_ref() == Some(before);
        if stable {
            let selected: HashSet<_> = selection.pairs.iter().map(DiffPair::key).collect();
            let (retained, stats): (Vec<DiffPair>, Vec<DiffStat>) = missing
                .iter()
                .zip(&batch.stats)
                .filter(|(pair, _)| selected.contains(&pair.key()))
                .map(|(pair, stat)| (pair.clone(), *stat))
                .unzip();
            if !retained.is_empty() {
                self.retain_statistics(
                    &retained,
                    &DiffBatch {
                        stats,
                        paths: Vec::new(),
                    },
                    selection.dependencies,
                    before,
                );
            }
        } else {
            crate::diagnostics::event(format_args!(
                "diff_admission status=rejected reason={}",
                if covered {
                    "context_changed"
                } else {
                    "uncovered_dependencies"
                }
            ));
        }
        if !stable && !values.is_empty() {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=context_changed"));
            return self.live_statistics(pairs, requested);
        }
        values.extend(
            missing
                .iter()
                .zip(batch.stats)
                .map(|(pair, stat)| (pair.key(), stat)),
        );
        assembled(pairs, &values, batch.paths)
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
        let selection = self.statistics_selection(&requested, &dependencies);
        crate::diagnostics::count("diff_validated_paths", selection.paths.len());
        if selection.pairs.is_empty() {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=dependency_limit"));
            return self.live_statistics(pairs, &requested);
        }
        let Some(before) = self.inner.diff_snapshot(&selection.paths) else {
            crate::diagnostics::event(format_args!("diff_cache=bypass reason=unsupported_context"));
            return self.live_statistics(pairs, &requested);
        };
        self.selected_statistics(pairs, &requested, &selection, &before)
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

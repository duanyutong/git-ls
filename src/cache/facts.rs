//! Successful object-ID-keyed facts and pure retention policy.
use crate::model::{CommitMeta, DiffStat};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

mod encoding;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Facts {
    pub(crate) bases: HashMap<String, Option<String>>,
    pub(crate) distances: HashMap<String, Option<usize>>,
    pub(crate) metadata: HashMap<String, CommitMeta>,
    #[serde(default)]
    pub(crate) diff: DiffFacts,
    #[serde(default)]
    recency: HashMap<String, u64>,
    #[serde(default)]
    clock: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DiffFacts {
    pub(crate) trees: HashMap<String, String>,
    pub(crate) scopes: BTreeMap<u64, DiffScope>,
    pub(crate) clock: u64,
    pub(crate) active: HashSet<(u64, String)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DiffScope {
    pub(crate) values: HashMap<String, DiffValue>,
    pub(crate) used: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiffValue {
    pub(crate) stat: DiffStat,
    pub(crate) paths: Rc<Vec<String>>,
    pub(crate) fingerprint: u64,
    pub(crate) configuration: u64,
    pub(crate) used: u64,
}

impl DiffValue {
    fn same_answer(&self, other: &Self) -> bool {
        self.stat == other.stat
            && self.paths == other.paths
            && self.fingerprint == other.fingerprint
            && self.configuration == other.configuration
    }
}

impl DiffFacts {
    fn same_answers(&self, other: &Self) -> bool {
        self.trees == other.trees
            && self.scopes.len() == other.scopes.len()
            && self.scopes.iter().all(|(id, scope)| {
                other.scopes.get(id).is_some_and(|previous| {
                    scope.values.len() == previous.values.len()
                        && scope.values.iter().all(|(key, value)| {
                            previous
                                .values
                                .get(key)
                                .is_some_and(|old| value.same_answer(old))
                        })
                })
            })
    }

    pub(crate) const fn tick(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    fn ordered_pairs(&self) -> Vec<(u64, String)> {
        let mut entries = Vec::new();
        let mut weights = HashMap::new();
        for (&scope, values) in &self.scopes {
            for (key, value) in &values.values {
                let pair = (scope, key.clone());
                let weight = *weights
                    .entry(std::rc::Rc::as_ptr(&value.paths))
                    .or_insert_with(|| dependency_weight(&value.paths));
                entries.push((
                    self.active.contains(&pair),
                    std::cmp::Reverse(weight),
                    value.used,
                    pair,
                ));
            }
        }
        entries.sort_unstable_by(|left, right| right.cmp(left));
        entries.into_iter().map(|(_, _, _, pair)| pair).collect()
    }

    fn prune_active(&mut self) {
        self.active.retain(|(context, key)| {
            self.scopes
                .get(context)
                .is_some_and(|scope| scope.values.contains_key(key))
        });
    }

    fn resize_pairs(
        &mut self,
        order: &[(u64, String)],
        discarded: &mut Vec<(u64, u64, String, DiffValue)>,
        size: usize,
    ) {
        let current = order.len().saturating_sub(discarded.len());
        if size < current {
            for (context, key) in order.iter().take(current).skip(size).rev() {
                let removed = self.scopes.get_mut(context).and_then(|scope| {
                    scope
                        .values
                        .remove(key)
                        .map(|value| (*context, scope.used, key.clone(), value))
                });
                discarded.extend(removed);
            }
        } else {
            for (context, used, key, value) in (current..size).filter_map(|_| discarded.pop()) {
                let scope = self.scopes.entry(context).or_default();
                scope.used = used;
                scope.values.insert(key, value);
            }
        }
        self.scopes.retain(|_, scope| !scope.values.is_empty());
    }

    pub(crate) fn compact(&mut self, limit: usize, recency: &HashMap<String, u64>) {
        retain(&mut self.trees, recency, limit);
        let mut scopes: Vec<_> = self
            .scopes
            .iter()
            .map(|(&id, scope)| {
                (
                    self.active.iter().any(|(context, _)| *context == id),
                    scope.used,
                    id,
                )
            })
            .collect();
        scopes.sort_unstable();
        for (_, _, id) in scopes.iter().rev().skip(4) {
            self.scopes.remove(id);
        }
        let retained: HashSet<_> = self.ordered_pairs().into_iter().take(limit).collect();
        self.scopes.retain(|id, scope| {
            scope
                .values
                .retain(|key, _| retained.contains(&(*id, key.clone())));
            !scope.values.is_empty()
        });
        self.prune_active();
    }
}

fn dependency_weight(paths: &[String]) -> usize {
    paths.iter().fold(0usize, |total, path| {
        path.bytes().fold(total.saturating_add(4), |total, byte| {
            total.saturating_add(match byte {
                b'"' | b'\\' => 4,
                b'\n' | b'\r' | b'\t' | 8 | 12 => 3,
                0..=31 => 7,
                _ => 1,
            })
        })
    })
}

fn retain<T>(map: &mut HashMap<String, T>, recency: &HashMap<String, u64>, limit: usize) {
    if map.len() <= limit {
        return;
    }
    let mut newest = std::collections::BinaryHeap::new();
    for key in map.keys() {
        newest.push(std::cmp::Reverse((
            recency.get(key).copied().unwrap_or(0),
            key,
        )));
        if newest.len() > limit {
            newest.pop();
        }
    }
    let retained: std::collections::HashSet<_> = newest
        .into_iter()
        .map(|std::cmp::Reverse((_, key))| key.clone())
        .collect();
    map.retain(|key, _| retained.contains(key));
}

fn extend_missing<K: Eq + std::hash::Hash, V>(
    target: &mut HashMap<K, V>,
    incoming: HashMap<K, V>,
) -> usize {
    let before = target.len();
    for (key, value) in incoming {
        target.entry(key).or_insert(value);
    }
    target.len().saturating_sub(before)
}

impl Facts {
    /// Merge only facts identified by the same immutable compatibility key.
    /// Newly calculated answers take precedence; unchanged loaded answers may adopt a concurrent writer's update.
    /// Active protection remains local.
    pub(crate) fn merge_concurrent(&mut self, incoming: Self, original: &Self) -> usize {
        let mut added = extend_missing(&mut self.bases, incoming.bases);
        added = added.saturating_add(extend_missing(&mut self.distances, incoming.distances));
        added = added.saturating_add(extend_missing(&mut self.metadata, incoming.metadata));
        self.clock = self.clock.max(incoming.clock);
        for (key, used) in incoming.recency {
            self.recency
                .entry(key)
                .and_modify(|value| *value = (*value).max(used))
                .or_insert(used);
        }
        self.diff.clock = self.diff.clock.max(incoming.diff.clock);
        added = added.saturating_add(extend_missing(&mut self.diff.trees, incoming.diff.trees));
        for (context, values) in incoming.diff.scopes {
            let scope = self.diff.scopes.entry(context).or_default();
            scope.used = scope.used.max(values.used);
            for (key, value) in values.values {
                match scope.values.entry(key) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(value);
                        added = added.saturating_add(1);
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        let unchanged = original
                            .diff
                            .scopes
                            .get(&context)
                            .and_then(|scope| scope.values.get(entry.key()))
                            .is_some_and(|before| entry.get().same_answer(before));
                        if unchanged && !entry.get().same_answer(&value) {
                            entry.insert(value);
                            added = added.saturating_add(1);
                        } else {
                            let used = entry.get().used.max(value.used);
                            entry.get_mut().used = used;
                        }
                    }
                }
            }
        }
        added
    }

    pub(crate) fn same_answers(&self, other: &Self) -> bool {
        self.bases == other.bases
            && self.distances == other.distances
            && self.metadata == other.metadata
            && self.diff.same_answers(&other.diff)
    }
    pub(crate) fn touch(&mut self, key: &str) {
        self.clock = self.clock.saturating_add(1);
        self.recency.insert(key.to_string(), self.clock);
    }

    /// Preserve graph answers before metadata and optional diff statistics.
    pub(crate) fn fit(&mut self, limit: usize, fits: &dyn Fn(&Self) -> bool) {
        self.compact(limit);
        if fits(self) {
            return;
        }
        if !self.diff.scopes.is_empty() {
            self.fit_statistics(&fits);
        }
        while !fits(self) {
            if !self.diff.trees.is_empty() {
                let tree_limit = self.diff.trees.len() / 2;
                retain(&mut self.diff.trees, &self.recency, tree_limit);
                continue;
            }
            if self.metadata.is_empty() {
                if self.bases.is_empty() && self.distances.is_empty() {
                    break;
                }
                self.compact(self.bases.len().max(self.distances.len()) / 2);
            } else {
                let metadata_limit = self.metadata.len() / 2;
                retain(&mut self.metadata, &self.recency, metadata_limit);
                self.compact(limit);
            }
        }
    }

    fn fit_statistics(&mut self, fits: &impl Fn(&Self) -> bool) {
        let order = self.diff.ordered_pairs();
        let mut discarded = Vec::new();
        let mut lower = 0;
        let mut upper = order.len();
        while lower < upper {
            let size = lower + (upper - lower).div_ceil(2);
            self.diff.resize_pairs(&order, &mut discarded, size);
            if fits(self) {
                lower = size;
            } else {
                upper = size.saturating_sub(1);
            }
        }
        self.diff.resize_pairs(&order, &mut discarded, lower);
        self.diff.prune_active();
    }

    pub(crate) fn compact(&mut self, limit: usize) {
        retain(&mut self.bases, &self.recency, limit);
        retain(&mut self.distances, &self.recency, limit);
        retain(&mut self.metadata, &self.recency, limit);
        self.diff.compact(limit, &self.recency);
        self.recency.retain(|key, _| {
            self.bases.contains_key(key)
                || self.distances.contains_key(key)
                || self.metadata.contains_key(key)
                || self.diff.trees.contains_key(key)
        });
    }
}

#[cfg(test)]
#[path = "tests/facts.rs"]
mod tests;

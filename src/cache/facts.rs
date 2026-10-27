//! Successful object-ID-keyed facts and pure retention policy.
use crate::model::{CommitMeta, DiffStat};
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiffFacts {
    pub(crate) trees: HashMap<String, String>,
    pub(crate) scopes: BTreeMap<u64, DiffScope>,
    pub(crate) clock: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiffScope {
    pub(crate) values: HashMap<String, DiffValue>,
    pub(crate) used: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiffValue {
    pub(crate) stat: DiffStat,
    pub(crate) paths: Vec<String>,
    pub(crate) fingerprint: u64,
    pub(crate) used: u64,
}

impl DiffFacts {
    fn same_answers(&self, other: &Self) -> bool {
        self.trees == other.trees
            && self.scopes.len() == other.scopes.len()
            && self.scopes.iter().all(|(id, scope)| {
                other.scopes.get(id).is_some_and(|previous| {
                    scope.values.len() == previous.values.len()
                        && scope.values.iter().all(|(key, value)| {
                            previous.values.get(key).is_some_and(|old| {
                                value.stat == old.stat
                                    && value.paths == old.paths
                                    && value.fingerprint == old.fingerprint
                            })
                        })
                })
            })
    }

    pub(crate) const fn tick(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    pub(crate) fn compact(&mut self, limit: usize, recency: &HashMap<String, u64>) {
        retain(&mut self.trees, recency, limit);
        let mut scopes: Vec<_> = self
            .scopes
            .iter()
            .map(|(&id, scope)| (scope.used, id))
            .collect();
        scopes.sort_unstable();
        for (_, id) in scopes.iter().rev().skip(4) {
            self.scopes.remove(id);
        }
        let mut entries: Vec<_> = self
            .scopes
            .iter()
            .flat_map(|(&scope, values)| {
                values
                    .values
                    .iter()
                    .map(move |(key, value)| (value.used, scope, key.clone()))
            })
            .collect();
        entries.sort_unstable();
        let retained: HashSet<_> = entries
            .into_iter()
            .rev()
            .take(limit)
            .map(|(_, scope, key)| (scope, key))
            .collect();
        self.scopes.retain(|id, scope| {
            scope
                .values
                .retain(|key, _| retained.contains(&(*id, key.clone())));
            !scope.values.is_empty()
        });
    }
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

impl Facts {
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
        while !fits(self) {
            if !self.diff.scopes.is_empty() || !self.diff.trees.is_empty() {
                if self.diff.scopes.len() > 1 {
                    let id = self
                        .diff
                        .scopes
                        .iter()
                        .min_by_key(|(_, scope)| scope.used)
                        .map(|(&id, _)| id)
                        .unwrap_or_default();
                    self.diff.scopes.remove(&id);
                } else {
                    self.diff = DiffFacts::default();
                }
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

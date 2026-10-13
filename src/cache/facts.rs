//! Successful object-ID-keyed facts and pure retention policy.
use crate::model::CommitMeta;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Facts {
    pub(crate) bases: HashMap<String, Option<String>>,
    pub(crate) distances: HashMap<String, Option<usize>>,
    pub(crate) metadata: HashMap<String, CommitMeta>,
    #[serde(default)]
    recency: HashMap<String, u64>,
    #[serde(default)]
    clock: u64,
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
    pub(crate) fn touch(&mut self, key: &str) {
        self.clock = self.clock.saturating_add(1);
        self.recency.insert(key.to_string(), self.clock);
    }

    /// Preserve graph answers before optional metadata when byte admission fails.
    pub(crate) fn fit(&mut self, limit: usize, fits: &dyn Fn(&Self) -> bool) {
        self.compact(limit);
        while !fits(self) {
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
        self.recency.retain(|key, _| {
            self.bases.contains_key(key)
                || self.distances.contains_key(key)
                || self.metadata.contains_key(key)
        });
    }
}

#[cfg(test)]
#[path = "tests/facts.rs"]
mod tests;

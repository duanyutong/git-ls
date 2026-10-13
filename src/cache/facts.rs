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

impl Facts {
    pub(crate) fn touch(&mut self, key: &str) {
        self.clock = self.clock.saturating_add(1);
        self.recency.insert(key.to_string(), self.clock);
    }

    pub(crate) fn compact(&mut self, limit: usize) {
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
mod tests {
    use super::*;
    #[test]
    fn compaction_retains_recently_reused_facts() {
        let mut facts = Facts::default();
        for key in ["old", "warm", "new"] {
            facts.bases.insert(key.to_string(), None);
            facts.distances.insert(key.to_string(), Some(1));
            facts
                .metadata
                .insert(key.to_string(), CommitMeta::new(key, 0, key));
            facts.touch(key);
        }
        facts.touch("warm");
        facts.recency.insert("discarded".to_string(), 100);
        facts.compact(1);
        assert_eq!(facts.bases.keys().collect::<Vec<_>>(), vec!["warm"]);
        assert_eq!(facts.distances.len(), 1);
        assert_eq!(facts.metadata.len(), 1);
        assert_eq!(facts.recency.len(), 1);
        let mut facts = Facts::default();
        facts.bases.insert("base".to_string(), None);
        facts.bases.insert("untouched".to_string(), None);
        facts.distances.insert("distance".to_string(), None);
        facts.metadata.insert(
            "metadata".to_string(),
            CommitMeta::new("metadata", 0, "meta"),
        );
        for key in ["base", "distance", "metadata", "removed"] {
            facts.touch(key);
        }
        facts.compact(1);
        assert_eq!(facts.recency.len(), 3);
    }
}

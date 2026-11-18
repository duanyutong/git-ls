//! Conservative dependency reuse between adjacent endpoint trees.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::{Dependencies, Session, Transition, invalid};
use crate::backend::DiffPair;
use crate::cache::facts::DEPENDENCY_REUSE_LIMIT;
use crate::error::Result;

#[derive(Clone)]
struct Previous {
    pair: DiffPair,
    paths: Rc<Vec<String>>,
    depth: u8,
    used: u64,
    context: u64,
}

impl Previous {
    fn rank(&self) -> (u64, std::cmp::Reverse<usize>, &str, u64) {
        (
            self.used,
            std::cmp::Reverse(self.paths.len()),
            &self.pair.head,
            self.context,
        )
    }
}

fn tree_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Session<'_> {
    fn previous_statistics(&self) -> HashMap<String, Previous> {
        let facts = self.facts.borrow();
        let mut previous: HashMap<String, Previous> = HashMap::new();
        for (&context, scope) in &facts.diff.scopes {
            for (key, value) in &scope.values {
                let Some((base, head)) = key.split_once(':') else {
                    continue;
                };
                if !tree_id(base) || !tree_id(head) {
                    continue;
                }
                let candidate = Previous {
                    pair: DiffPair {
                        base: Some(base.to_owned()),
                        head: head.to_owned(),
                    },
                    paths: value.paths.clone(),
                    depth: value.dependency_depth,
                    used: value.used,
                    context,
                };
                if previous
                    .get(base)
                    .is_none_or(|old| candidate.rank() > old.rank())
                {
                    previous.insert(base.to_owned(), candidate);
                }
            }
        }
        previous
    }

    fn discover_direct(&self, pairs: &[DiffPair]) -> Result<Dependencies> {
        if pairs.is_empty() {
            return Ok(HashMap::new());
        }
        let discovered = self.inner.diff_paths(pairs)?;
        if discovered.len() != pairs.len() {
            return Err(invalid());
        }
        Ok(pairs
            .iter()
            .zip(discovered)
            .map(|(pair, mut paths)| {
                paths.sort_unstable();
                paths.dedup();
                let key = pair.key();
                self.dependency_depths.borrow_mut().insert(key.clone(), 0);
                (key, Rc::new(paths))
            })
            .collect())
    }

    pub(super) fn discover_statistics_paths(&self, unknown: &[DiffPair]) -> Result<Dependencies> {
        let previous = self.previous_statistics();
        let mut direct = Vec::new();
        let mut requests: HashMap<DiffPair, Vec<_>> = HashMap::new();
        for pair in unknown {
            let candidate = pair
                .base
                .as_ref()
                .and_then(|base| previous.get(base))
                .filter(|candidate| candidate.depth < DEPENDENCY_REUSE_LIMIT);
            let Some(candidate) = candidate else {
                direct.push(pair.clone());
                continue;
            };
            let range = DiffPair {
                base: Some(candidate.pair.head.clone()),
                head: pair.head.clone(),
            };
            requests.entry(range).or_default().push((pair, candidate));
        }
        if requests.is_empty() {
            return self.discover_direct(unknown);
        }
        let dependent_count = requests.values().map(Vec::len).sum::<usize>();
        let mut requests: Vec<_> = requests.into_iter().collect();
        requests.sort_unstable_by(|left, right| {
            (&left.0.base, &left.0.head).cmp(&(&right.0.base, &right.0.head))
        });
        let ranges: Vec<_> = requests.iter().map(|(range, _)| range.clone()).collect();
        let Ok(discovered) = self.inner.diff_paths(&ranges) else {
            crate::diagnostics::count("diff_dependency_reuse_fallbacks", dependent_count);
            crate::diagnostics::event(format_args!(
                "diff_dependencies strategy=direct reason=previous_tree_unavailable"
            ));
            return self.discover_direct(unknown);
        };
        if discovered.len() != requests.len() {
            return Err(invalid());
        }
        let mut result = self.discover_direct(&direct)?;
        for ((_, dependents), mut changed) in requests.into_iter().zip(discovered) {
            changed.sort_unstable();
            changed.dedup();
            let changed = Rc::new(changed);
            for (pair, candidate) in dependents {
                let transition = Transition {
                    previous: candidate.pair.clone(),
                    paths: changed.clone(),
                };
                let paths = Rc::new(
                    candidate
                        .paths
                        .iter()
                        .chain(transition.paths.iter())
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                );
                let depth = candidate.depth.saturating_add(1);
                let key = pair.key();
                crate::diagnostics::count("diff_dependency_reuses", 1);
                crate::diagnostics::event(format_args!(
                    "diff_dependencies pair={key} strategy=transition previous={} changed_paths={} depth={depth}",
                    transition.previous.head,
                    transition.paths.len()
                ));
                self.dependency_depths
                    .borrow_mut()
                    .insert(key.clone(), depth);
                self.diff_transitions
                    .borrow_mut()
                    .insert(key.clone(), transition);
                result.insert(key, paths);
            }
        }
        Ok(result)
    }
}

//! Interned persistent dependencies with bounded expansion and shared decoded sets.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem::size_of;
use std::rc::Rc;

use serde::{Deserialize, Serialize, de::Error as _, ser::Error as _};

use super::{DEPENDENCY_REUSE_LIMIT, DiffFacts, DiffScope, DiffValue};
use crate::model::DiffStat;

// The encoded entry remains limited to 1 MiB; pooling must not permit unbounded expansion when a small entry refers repeatedly to long dependency names.
const EXPANDED_PATH_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct WireDiff<T, P, K: Ord, I = usize> {
    trees: T,
    paths: Vec<P>,
    dependencies: Vec<Vec<usize>>,
    scopes: BTreeMap<u64, WireScope<K, I>>,
    clock: u64,
}

#[derive(Serialize, Deserialize)]
struct WireScope<K: Ord, I = usize> {
    values: BTreeMap<K, WireValue<I>>,
    used: u64,
}

#[derive(Serialize, Deserialize)]
struct WireValue<I = usize> {
    stat: DiffStat,
    paths: I,
    fingerprint: u64,
    configuration: u64,
    dependency_depth: u8,
    used: u64,
}

const fn bounded(contexts: usize, values: usize) -> bool {
    contexts <= 4 && values <= 4096
}

#[derive(Clone)]
struct DependencyId(Rc<Cell<usize>>);

impl Serialize for DependencyId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.get().serialize(serializer)
    }
}

#[derive(Default)]
struct Pools<'a> {
    paths: Vec<&'a str>,
    path_ids: HashMap<&'a str, usize>,
    dependencies: BTreeMap<Vec<usize>, DependencyId>,
    shared: HashMap<(*const String, usize), DependencyId>,
    expanded: usize,
}

type PackedDependencies<'a> = (Vec<&'a str>, Vec<Vec<usize>>);

impl<'a> Pools<'a> {
    fn new(facts: &'a DiffFacts) -> Self {
        let mut seen = HashSet::new();
        let mut paths: Vec<_> = facts
            .scopes
            .values()
            .flat_map(|scope| scope.values.values())
            .filter(|value| seen.insert(Rc::as_ptr(&value.paths)))
            .flat_map(|value| value.paths.iter().map(String::as_str))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        paths.sort_unstable();
        let path_ids = paths
            .iter()
            .enumerate()
            .map(|(id, &path)| (path, id))
            .collect();
        Self {
            paths,
            path_ids,
            ..Self::default()
        }
    }

    fn intern(&mut self, paths: &'a [String]) -> Result<DependencyId, &'static str> {
        let identity = (paths.as_ptr(), paths.len());
        if let Some(id) = self.shared.get(&identity) {
            return Ok(id.clone());
        }
        let indices: Vec<_> = paths
            .iter()
            .map(|path| {
                self.path_ids
                    .get(path.as_str())
                    .copied()
                    .ok_or("invalid dependency path")
            })
            .collect::<Result<_, _>>()?;
        let next = self.dependencies.len();
        let id = match self.dependencies.entry(indices) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.get().clone(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                self.expanded = paths.iter().fold(self.expanded, |total, path| {
                    total
                        .saturating_add(path.len())
                        .saturating_add(size_of::<String>())
                });
                if self.expanded > EXPANDED_PATH_LIMIT {
                    return Err("expanded dependencies exceed memory bound");
                }
                let id = DependencyId(Rc::new(Cell::new(next)));
                entry.insert(id.clone());
                id
            }
        };
        self.shared.insert(identity, id.clone());
        Ok(id)
    }

    fn finish(self) -> PackedDependencies<'a> {
        let dependencies = self
            .dependencies
            .into_iter()
            .enumerate()
            .map(|(id, (paths, reference))| {
                reference.0.set(id);
                paths
            })
            .collect();
        (self.paths, dependencies)
    }
}

impl Serialize for DiffFacts {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !bounded(
            self.scopes.len(),
            self.scopes.values().map(|scope| scope.values.len()).sum(),
        ) {
            return Err(S::Error::custom("statistics exceed retention bounds"));
        }
        let mut pools = Pools::new(self);
        let mut scopes = BTreeMap::new();
        for (&context, scope) in &self.scopes {
            let mut values = BTreeMap::new();
            for (key, value) in &scope.values {
                if value.dependency_depth > DEPENDENCY_REUSE_LIMIT {
                    return Err(S::Error::custom("dependency proof depth exceeds bound"));
                }
                let id = pools
                    .intern(value.paths.as_slice())
                    .map_err(S::Error::custom)?;
                values.insert(
                    key.as_str(),
                    WireValue {
                        stat: value.stat,
                        paths: id,
                        fingerprint: value.fingerprint,
                        configuration: value.configuration,
                        dependency_depth: value.dependency_depth,
                        used: value.used,
                    },
                );
            }
            scopes.insert(
                context,
                WireScope {
                    values,
                    used: scope.used,
                },
            );
        }
        let (paths, dependencies) = pools.finish();
        WireDiff {
            trees: &self.trees,
            paths,
            dependencies,
            scopes,
            clock: self.clock,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiffFacts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire: WireDiff<HashMap<String, String>, String, String> =
            WireDiff::deserialize(deserializer)?;
        if !bounded(
            wire.scopes.len(),
            wire.scopes
                .values()
                .map(|scope| scope.values.len())
                .sum::<usize>()
                .max(wire.dependencies.len()),
        ) {
            return Err(D::Error::custom("statistics exceed retention bounds"));
        }
        let mut dependencies = Vec::with_capacity(wire.dependencies.len());
        let mut expanded = 0usize;
        for indices in wire.dependencies {
            expanded = expanded.saturating_add(indices.len().saturating_mul(size_of::<String>()));
            if expanded > EXPANDED_PATH_LIMIT {
                return Err(D::Error::custom(
                    "expanded dependencies exceed memory bound",
                ));
            }
            let mut paths = Vec::with_capacity(indices.len());
            for id in indices {
                let path = wire
                    .paths
                    .get(id)
                    .ok_or_else(|| D::Error::custom("invalid dependency path"))?;
                expanded = expanded.saturating_add(path.len());
                if expanded > EXPANDED_PATH_LIMIT {
                    return Err(D::Error::custom(
                        "expanded dependencies exceed memory bound",
                    ));
                }
                paths.push(path.clone());
            }
            dependencies.push(Rc::new(paths));
        }
        let mut scopes = BTreeMap::new();
        for (context, scope) in wire.scopes {
            let mut values = HashMap::with_capacity(scope.values.len());
            for (key, value) in scope.values {
                if value.dependency_depth > DEPENDENCY_REUSE_LIMIT {
                    return Err(D::Error::custom("dependency proof depth exceeds bound"));
                }
                let paths = dependencies
                    .get(value.paths)
                    .ok_or_else(|| D::Error::custom("invalid dependency set"))?
                    .clone();
                values.insert(
                    key,
                    DiffValue {
                        stat: value.stat,
                        paths,
                        fingerprint: value.fingerprint,
                        configuration: value.configuration,
                        dependency_depth: value.dependency_depth,
                        used: value.used,
                    },
                );
            }
            scopes.insert(
                context,
                DiffScope {
                    values,
                    used: scope.used,
                },
            );
        }
        Ok(Self {
            trees: wire.trees,
            scopes,
            clock: wire.clock,
            active: HashSet::default(),
        })
    }
}

#[cfg(test)]
#[path = "../tests/encoding.rs"]
mod tests;

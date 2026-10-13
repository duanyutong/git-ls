use std::collections::{HashMap, HashSet};

use crate::backend::graph::{descendant_counts, proven_bases};
use crate::error::{GitLsError, Result};

use super::GixBackend;

type Oid = gix::ObjectId;
const PARENT_CACHE_LIMIT: usize = 65_536;

pub(super) struct TipAnalysis {
    pub(super) main: String,
    pub(super) unmerged: Vec<String>,
    pub(super) heads: Vec<String>,
    pub(super) bases: HashMap<String, Option<String>>,
}

impl GixBackend {
    pub(super) fn effective_parents(&self, id: Oid, mut parents: Vec<Oid>) -> Vec<Oid> {
        if self.shallow.contains(&id) {
            parents.clear();
        }
        parents
    }

    pub(super) fn remember_parents(&self, id: Oid, parents: &[Oid]) {
        let mut cache = self.parents.borrow_mut();
        if cache.len() < PARENT_CACHE_LIMIT {
            cache.entry(id).or_insert_with(|| parents.to_vec());
        }
    }

    fn parents(&self, id: Oid) -> Result<Vec<Oid>> {
        if let Some(parents) = self.parents.borrow().get(&id) {
            crate::diagnostics::count("parent_cache_hits", 1);
            return Ok(parents.clone());
        }
        crate::diagnostics::count("parent_loads", 1);
        let commit = self
            .repo
            .find_commit(id)
            .map_err(|error| GitLsError::gix("read parents", error))?;
        let parents: Vec<_> = commit.parent_ids().map(gix::Id::detach).collect();
        self.remember_parents(id, &parents);
        Ok(parents)
    }

    pub(super) fn analyse_tips(&self, main: &str, tips: &[String]) -> Result<TipAnalysis> {
        let _perf = crate::diagnostics::operation("select_tips");
        let main_id = Self::object_id(main)?;
        let candidates: HashSet<_> = tips
            .iter()
            .map(|tip| Self::object_id(tip))
            .collect::<Result<_>>()?;
        let mut unmerged = HashSet::new();
        let mut intermediate = HashSet::new();
        let mut vertices = 0;
        let mut edges = 0;
        let mut retained = Some(Vec::new());
        if !candidates.is_empty() {
            let walk = self
                .repo
                .rev_walk(candidates.iter().copied())
                .with_hidden([main_id])
                .all()
                .map_err(|error| GitLsError::gix("select branch tips", error))?;
            for info in walk {
                let info = info.map_err(|error| GitLsError::gix("read selection graph", error))?;
                vertices += 1;
                let parents = self.effective_parents(info.id, info.parent_ids.into_vec());
                edges += parents.len();
                self.remember_parents(info.id, &parents);
                if let Some(nodes) = retained.as_mut() {
                    if nodes.len() < PARENT_CACHE_LIMIT {
                        nodes.push((info.id, parents.clone()));
                    } else {
                        // A partial range cannot prove which edges are boundaries.
                        retained = None;
                    }
                }
                if candidates.contains(&info.id) {
                    unmerged.insert(info.id);
                }
                intermediate.extend(
                    parents
                        .iter()
                        .filter(|parent| candidates.contains(*parent))
                        .copied(),
                );
            }
        }
        let proven = retained
            .as_ref()
            .map_or_else(HashMap::new, |nodes| proven_bases(nodes));
        let bases = unmerged
            .difference(&intermediate)
            .filter_map(|head| {
                proven
                    .get(head)
                    .map(|base| (head.to_string(), base.map(|id| id.to_string())))
            })
            .collect();
        let mut heads: Vec<_> = unmerged
            .difference(&intermediate)
            .map(ToString::to_string)
            .collect();
        let mut unmerged: Vec<_> = unmerged.into_iter().map(|id| id.to_string()).collect();
        heads.sort();
        unmerged.sort();
        crate::diagnostics::count("selection_vertices", vertices);
        crate::diagnostics::count("selection_edges", edges);
        crate::diagnostics::maximum("peak_parent_entries", self.parents.borrow().len());
        crate::diagnostics::event(format_args!("selection_strategy=shared_range"));
        Ok(TipAnalysis {
            main: main.to_string(),
            unmerged,
            heads,
            bases,
        })
    }

    pub(super) fn distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        let _perf = crate::diagnostics::operation("main_distances");
        let main_id = Self::object_id(main)?;
        let ids: Vec<_> = bases
            .iter()
            .map(|base| Self::object_id(base))
            .collect::<Result<_>>()?;
        let wanted: HashSet<_> = ids.iter().copied().collect();
        if wanted.is_empty() {
            return Ok(HashMap::new());
        }
        if self
            .repo
            .is_shallow()
            .map_err(|source| GitLsError::gix("read shallow boundary", source))?
        {
            return self.dag_distances(main, bases, &ids);
        }
        let mut found = HashMap::new();
        let mut cursor = main_id;
        let mut distance = 0;
        loop {
            if wanted.contains(&cursor) {
                found.insert(cursor, distance);
            }
            if found.len() == wanted.len() {
                break;
            }
            let parents = self.parents(cursor)?;
            match parents.as_slice() {
                [] => break,
                [parent] => {
                    cursor = *parent;
                    distance += 1;
                }
                _ => return self.dag_distances(main, bases, &ids),
            }
        }
        crate::diagnostics::count("main_vertices", distance + 1);
        crate::diagnostics::maximum("peak_parent_entries", self.parents.borrow().len());
        crate::diagnostics::event(format_args!(
            "main_distance_strategy=linear visited_commits={}",
            distance + 1
        ));
        Ok(bases
            .iter()
            .zip(ids)
            .map(|(base, id)| (base.clone(), found.get(&id).copied()))
            .collect())
    }

    fn dag_distances(
        &self,
        main: &str,
        bases: &[String],
        ids: &[Oid],
    ) -> Result<HashMap<String, Option<usize>>> {
        let mut boundary = ids.first().copied();
        for id in ids.iter().skip(1) {
            let Some(previous) = boundary else { break };
            let common = self
                .repo
                .merge_bases_many(previous, &[*id])
                .map_err(|error| GitLsError::gix("find common distance boundary", error))?;
            boundary = common.first().map(|id| id.detach());
        }
        let main_id = Self::object_id(main)?;
        let mut walk = self.repo.rev_walk([main_id]);
        if let Some(boundary) = boundary {
            walk = walk.with_hidden([boundary]);
        }
        let mut nodes = Vec::new();
        for info in walk
            .all()
            .map_err(|error| GitLsError::gix("walk main history", error))?
        {
            let info = info.map_err(|error| GitLsError::gix("read main history", error))?;
            let parents = self.effective_parents(info.id, info.parent_ids.into_vec());
            self.remember_parents(info.id, &parents);
            nodes.push((info.id, parents));
        }
        crate::diagnostics::count("main_vertices", nodes.len());
        crate::diagnostics::event(format_args!(
            "main_distance_strategy=dag bases={}",
            bases.len()
        ));
        let counts = descendant_counts(&nodes, ids);
        Ok(bases
            .iter()
            .zip(counts)
            .map(|(base, count)| {
                (
                    base.clone(),
                    if base == main {
                        Some(0)
                    } else {
                        (count > 0).then_some(count)
                    },
                )
            })
            .collect())
    }
}

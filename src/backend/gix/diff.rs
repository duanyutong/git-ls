//! Attribute-independent tree traversal for statistics dependencies.

use std::collections::{BTreeMap, BTreeSet};

use super::GixBackend;
use crate::backend::DiffPair;
use crate::error::{GitLsError, Result};

type Entries = BTreeMap<Vec<u8>, (gix::objs::tree::EntryMode, gix::ObjectId)>;

impl GixBackend {
    fn tree_entries(&self, id: Option<gix::ObjectId>) -> Result<Entries> {
        let Some(id) = id else {
            return Ok(BTreeMap::new());
        };
        let tree = self
            .repo
            .find_tree(id)
            .map_err(|error| GitLsError::gix("read dependency tree", error))?;
        let mut entries = BTreeMap::new();
        for entry in tree.iter() {
            let entry = entry.map_err(|error| GitLsError::gix("read dependency entry", error))?;
            entries.insert(entry.filename().to_vec(), (entry.mode(), entry.object_id()));
        }
        Ok(entries)
    }

    pub(super) fn tree_paths(&self, pair: &DiffPair) -> Result<Vec<String>> {
        let base = pair.base.as_deref().map(Self::object_id).transpose()?;
        let head = Some(Self::object_id(&pair.head)?);
        let mut pending = vec![(base, head, Vec::new())];
        let mut paths = BTreeSet::new();
        while let Some((base, head, prefix)) = pending.pop() {
            if base == head {
                continue;
            }
            let before = self.tree_entries(base)?;
            let after = self.tree_entries(head)?;
            let names: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            for name in names {
                let old = before.get(name);
                let new = after.get(name);
                if old == new {
                    continue;
                }
                let mut path = prefix.clone();
                path.extend_from_slice(name);
                if old.into_iter().chain(new).any(|(mode, _)| !mode.is_tree()) {
                    paths.insert(String::from_utf8_lossy(&path).into_owned());
                }
                let old_tree = old.filter(|(mode, _)| mode.is_tree()).map(|(_, id)| *id);
                let new_tree = new.filter(|(mode, _)| mode.is_tree()).map(|(_, id)| *id);
                if old_tree.is_some() || new_tree.is_some() {
                    path.push(b'/');
                    pending.push((old_tree, new_tree, path));
                }
            }
        }
        crate::diagnostics::count("diff_discovered_paths", paths.len());
        Ok(paths.into_iter().collect())
    }
}

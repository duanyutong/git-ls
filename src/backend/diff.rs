use std::collections::{BTreeSet, HashMap};

use super::{DiffStatBackend, GitCommand};
use crate::error::{GitLsError, Result};
use crate::model::DiffStat;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DiffPair {
    pub(crate) base: Option<String>,
    pub(crate) head: String,
}

impl DiffPair {
    pub(crate) fn key(&self) -> String {
        format!("{}:{}", self.base.as_deref().unwrap_or_default(), self.head)
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct DiffBatch {
    pub(crate) stats: Vec<DiffStat>,
    pub(crate) paths: Vec<String>,
}

pub(super) const DIFF_ARGS: &[&str] = &[
    "diff-tree",
    "--stdin",
    "-r",
    "--numstat",
    "-z",
    "--no-ext-diff",
    "--no-textconv",
    "--find-renames",
    "--no-relative",
    "--no-color",
];

fn invalid() -> GitLsError {
    GitLsError::git_command(
        "diff-tree --numstat",
        "invalid or overflowing batched diff output",
    )
}

pub(super) fn commit_ids(pairs: &[DiffPair]) -> BTreeSet<&str> {
    pairs
        .iter()
        .flat_map(|pair| {
            pair.base
                .as_deref()
                .into_iter()
                .chain(Some(pair.head.as_str()))
        })
        .collect()
}

impl<T: GitCommand + ?Sized> DiffStatBackend for T {
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        if pairs.is_empty() {
            return Ok(DiffBatch::default());
        }
        let ids = commit_ids(pairs);
        let mut input = String::new();
        for id in &ids {
            input.push_str(id);
            input.push_str("^{tree}\n");
        }
        let output = self.run_input(&["cat-file", "--batch-check=%(objectname)"], &input)?;
        let trees = parse_trees(ids.into_iter(), &output)?;
        diff_with_trees(self, pairs, &trees)
    }

    fn diff_context(&self, paths: &[String]) -> Option<u64> {
        GitCommand::diff_context(self, paths)
    }
}

fn parse_trees<'a>(
    ids: impl Iterator<Item = &'a str>,
    output: &str,
) -> Result<HashMap<String, String>> {
    let mut records = output.lines();
    let mut trees = HashMap::new();
    for id in ids {
        let tree = records.next().ok_or_else(invalid)?;
        if !matches!(tree.len(), 40 | 64) || !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        trees.insert(id.to_string(), tree.to_string());
    }
    if records.next().is_some() {
        return Err(invalid());
    }
    Ok(trees)
}

pub(super) fn diff_with_trees(
    git: &(impl GitCommand + ?Sized),
    pairs: &[DiffPair],
    trees: &HashMap<String, String>,
) -> Result<DiffBatch> {
    let _perf = crate::diagnostics::operation("diff_stat");
    let empty = if pairs.iter().any(|pair| pair.base.is_none()) {
        git.empty_tree()?
    } else {
        String::new()
    };
    let mut requests = Vec::new();
    let mut indices = HashMap::new();
    let mut positions = Vec::with_capacity(pairs.len());
    let mut input = String::new();
    for pair in pairs {
        let base = match &pair.base {
            Some(base) => trees.get(base).ok_or_else(invalid)?.as_str(),
            None => empty.trim(),
        };
        let head = trees.get(&pair.head).ok_or_else(invalid)?;
        if base == head {
            positions.push(None);
            crate::diagnostics::count("diff_equal_tree_skips", 1);
            continue;
        }
        let key = (base, head.as_str());
        let next = indices.len();
        let index = *indices.entry(key).or_insert_with(|| {
            let header = format!("{base} {head}\n");
            input.push_str(&header);
            requests.push(header);
            next
        });
        positions.push(Some(index));
    }
    crate::diagnostics::count("diff_tree_pairs", requests.len());
    let mut batch = if requests.is_empty() {
        DiffBatch::default()
    } else {
        crate::diagnostics::count("diff_batch_launches", 1);
        let output = git.run_input(DIFF_ARGS, &input)?;
        parse_batch(&requests, &output)?
    };
    batch.stats = positions
        .into_iter()
        .map(|position| {
            position
                .and_then(|index| batch.stats.get(index).copied())
                .unwrap_or_default()
        })
        .collect();
    Ok(batch)
}

fn parse_batch(headers: &[String], mut output: &str) -> Result<DiffBatch> {
    let mut batch = DiffBatch::default();
    let mut paths = BTreeSet::new();
    for (index, header) in headers.iter().enumerate() {
        output = output.strip_prefix(header).ok_or_else(invalid)?;
        let mut stat = DiffStat::default();
        while !output.is_empty()
            && !headers
                .get(index + 1)
                .is_some_and(|next| output.starts_with(next))
        {
            let (record, rest) = output.split_once('\0').ok_or_else(invalid)?;
            output = rest;
            let (added, rest) = record.split_once('\t').ok_or_else(invalid)?;
            let (deleted, path) = rest.split_once('\t').ok_or_else(invalid)?;
            if path.is_empty() {
                for _ in 0..2 {
                    let (path, rest) = output.split_once('\0').ok_or_else(invalid)?;
                    paths.insert(path.to_string());
                    output = rest;
                }
            } else {
                paths.insert(path.to_string());
            }
            if added == "-" && deleted == "-" {
                continue;
            }
            stat.added = stat
                .added
                .checked_add(added.parse().map_err(|_| invalid())?)
                .ok_or_else(invalid)?;
            stat.deleted = stat
                .deleted
                .checked_add(deleted.parse().map_err(|_| invalid())?)
                .ok_or_else(invalid)?;
        }
        batch.stats.push(stat);
    }
    if !output.is_empty() {
        return Err(invalid());
    }
    batch.paths = paths.into_iter().collect();
    Ok(batch)
}

#[cfg(test)]
#[path = "tests/diff.rs"]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};

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

/// Effective inputs for one capture; path fingerprints permit subset validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiffContext {
    pub(crate) scope: u64,
    pub(crate) configuration: u64,
    pub(crate) attributes: BTreeMap<String, u64>,
}

impl DiffContext {
    pub(crate) const fn uniform(configuration: u64) -> Self {
        Self {
            scope: 0,
            configuration,
            attributes: BTreeMap::new(),
        }
    }

    pub(crate) fn fingerprint(&self, paths: &[String]) -> u64 {
        crate::cache::hash(&(
            self.configuration,
            paths
                .iter()
                .map(|path| (path, self.attributes.get(path)))
                .collect::<Vec<_>>(),
        ))
    }
}

pub(super) fn configuration_fingerprint(config: &str) -> u64 {
    let records: Vec<_> = config
        .split_terminator('\0')
        .filter(|record| {
            let key = record
                .split_once('\n')
                .map_or(*record, |(key, _)| key)
                .to_ascii_lowercase();
            !key.starts_with("git-ls.") && !matches!(key.as_str(), "core.editor" | "core.pager")
        })
        .collect();
    crate::cache::hash(&records)
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

const PATH_ARGS: &[&str] = &[
    "diff-tree",
    "--stdin",
    "-r",
    "--raw",
    "--no-abbrev",
    "-z",
    "--no-renames",
    "--no-ext-diff",
    "--no-textconv",
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

    fn diff_snapshot(&self, paths: &[String]) -> Option<DiffContext> {
        GitCommand::diff_snapshot(self, paths)
    }

    fn diff_tree_pairs(
        &self,
        pairs: &[DiffPair],
        trees: &mut HashMap<String, String>,
    ) -> Result<Option<Vec<DiffPair>>> {
        let ids: Vec<_> = commit_ids(pairs)
            .into_iter()
            .filter(|id| !trees.contains_key(*id))
            .collect();
        if !ids.is_empty() {
            let mut input = String::new();
            for id in &ids {
                input.push_str(id);
                input.push_str("^{tree}\n");
            }
            let output = self.run_input(&["cat-file", "--batch-check=%(objectname)"], &input)?;
            trees.extend(parse_trees(ids.into_iter(), &output)?);
        }
        let empty = if pairs.iter().any(|pair| pair.base.is_none()) {
            self.empty_tree()?.trim().to_string()
        } else {
            String::new()
        };
        resolved_pairs(pairs, trees, &empty).map(Some)
    }

    fn diff_paths(&self, pairs: &[DiffPair]) -> Result<Vec<Vec<String>>> {
        let headers: Vec<_> = pairs
            .iter()
            .map(|pair| {
                format!(
                    "{} {}\n",
                    pair.base.as_deref().unwrap_or_default(),
                    pair.head
                )
            })
            .collect();
        if headers.is_empty() {
            return Ok(Vec::new());
        }
        let output = self.run_input(PATH_ARGS, &headers.concat())?;
        parse_paths(&headers, &output)
    }

    fn diff_tree_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        tree_stats(self, pairs)
    }
}

pub(super) fn resolved_pairs(
    pairs: &[DiffPair],
    trees: &HashMap<String, String>,
    empty: &str,
) -> Result<Vec<DiffPair>> {
    pairs
        .iter()
        .map(|pair| {
            Ok(DiffPair {
                base: Some(match &pair.base {
                    Some(id) => trees.get(id).ok_or_else(invalid)?.clone(),
                    None => empty.to_string(),
                }),
                head: trees.get(&pair.head).ok_or_else(invalid)?.clone(),
            })
        })
        .collect()
}

pub(super) fn tree_stats(
    git: &(impl GitCommand + ?Sized),
    pairs: &[DiffPair],
) -> Result<DiffBatch> {
    let trees = commit_ids(pairs)
        .into_iter()
        .map(|id| (id.to_string(), id.to_string()))
        .collect();
    diff_with_trees(git, pairs, &trees)
}

fn parse_paths(headers: &[String], mut output: &str) -> Result<Vec<Vec<String>>> {
    let mut result = Vec::with_capacity(headers.len());
    for (index, header) in headers.iter().enumerate() {
        output = output.strip_prefix(header).ok_or_else(invalid)?;
        let mut paths = Vec::new();
        while !output.is_empty()
            && !headers
                .get(index + 1)
                .is_some_and(|next| output.starts_with(next))
        {
            let (record, rest) = output.split_once('\0').ok_or_else(invalid)?;
            if !record.starts_with(':') {
                return Err(invalid());
            }
            let (path, rest) = rest.split_once('\0').ok_or_else(invalid)?;
            paths.push(path.to_string());
            output = rest;
        }
        result.push(paths);
    }
    if !output.is_empty() {
        return Err(invalid());
    }
    Ok(result)
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

use std::collections::{HashMap, HashSet};

use crate::error::{GitLsError, Result};
use crate::model::CommitMeta;

use super::traits::CommitMetadataBackend;

pub(super) const GIT_SHOW_COMMIT_META_ARG: &str = "--format=%H%x00%ct%x00%s%x1e";

pub(crate) fn get_commit_meta<G: CommitMetadataBackend + ?Sized>(
    git: &G,
    oid: &str,
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<CommitMeta> {
    if let Some(meta) = cache.get(oid) {
        return Ok(meta.clone());
    }

    git.cache_commit_metas(&[oid], cache)?;
    cache
        .get(oid)
        .cloned()
        .ok_or_else(|| GitLsError::unexpected_git_show(oid))
}

pub(super) fn missing_commit_aliases<'a>(
    oids: &'a [&str],
    cache: &HashMap<String, CommitMeta>,
) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    oids.iter()
        .copied()
        .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
        .collect()
}

pub(super) fn git_show_commit_records(output: &str) -> Vec<&str> {
    output
        .split('\x1e')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .map(|record| record.strip_suffix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .collect()
}

pub(super) fn parse_shell_commit_meta(alias: &str, record: &str) -> Result<CommitMeta> {
    let parts: Vec<&str> = record.splitn(3, '\0').collect();
    if parts.len() != 3 {
        return Err(GitLsError::unexpected_git_show(alias));
    }

    let timestamp = parts[1]
        .parse()
        .map_err(|source| GitLsError::invalid_commit_timestamp(alias, source))?;
    Ok(CommitMeta::new(parts[0], timestamp, parts[2]))
}

pub(super) fn insert_commit_meta(
    cache: &mut HashMap<String, CommitMeta>,
    alias: &str,
    meta: CommitMeta,
) {
    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
}

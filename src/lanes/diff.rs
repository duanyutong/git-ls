use std::collections::HashMap;

use crate::backend::{DiffPair, DiffStatBackend};
use crate::error::{GitLsError, Result};
use crate::model::LaneGroup;

pub(crate) fn populate_diff_stats(
    groups: &mut [LaneGroup],
    git: &dyn DiffStatBackend,
    enabled: bool,
) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    let _perf = crate::diagnostics::stage("diff_stats");
    let mut indices = HashMap::new();
    let mut pairs = Vec::new();
    let mut assignments = Vec::new();
    for group in groups {
        for lane in &mut group.lanes {
            let mut base = lane.base_oid.clone();
            for point in lane.branch_points.iter_mut().rev() {
                let pair = DiffPair {
                    base,
                    head: point.oid.clone(),
                };
                base = Some(point.oid.clone());
                if point.main_history {
                    continue;
                }
                let next = pairs.len();
                let index = *indices.entry(pair.clone()).or_insert_with(|| {
                    pairs.push(pair);
                    next
                });
                assignments.push((point, index));
            }
        }
    }
    crate::diagnostics::count("diff_requested_pairs", assignments.len());
    crate::diagnostics::count("diff_unique_pairs", pairs.len());
    if pairs.is_empty() {
        return Ok(());
    }
    let batch = git.diff_stats(&pairs)?;
    if batch.stats.len() != pairs.len() {
        return Err(GitLsError::git_command(
            "diff statistics",
            "backend returned an incorrect result count",
        ));
    }
    for (point, index) in assignments {
        point.diff_stat = batch.stats.get(index).copied();
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/diff.rs"]
mod tests;

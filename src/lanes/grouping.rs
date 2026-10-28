use super::observer::{AnalysisObserver, AnalysisStage};
use crate::backend::{GitBackend, get_commit_meta};
use crate::cli::Order;
use crate::error::Result;
use crate::model::{CommitMeta, Lane, LaneGroup, RepositorySnapshot};
use std::collections::HashMap;
pub(crate) fn ordered_lanes(mut lanes: Vec<Lane>, order: Order) -> Vec<Lane> {
    lanes.sort_by(|lhs, rhs| {
        match order {
            Order::Newest => rhs.head_timestamp.cmp(&lhs.head_timestamp),
            Order::Oldest => lhs.head_timestamp.cmp(&rhs.head_timestamp),
        }
        .then_with(|| lhs.head_oid.cmp(&rhs.head_oid))
    });
    lanes
}

pub(super) fn grouped_by_base(lanes: Vec<Lane>) -> Vec<(Option<String>, Vec<Lane>)> {
    let mut groups: HashMap<Option<String>, Vec<Lane>> = HashMap::new();
    for lane in lanes {
        groups.entry(lane.base_oid.clone()).or_default().push(lane);
    }

    groups.into_iter().collect()
}

pub(crate) fn build_lane_groups(
    git: &dyn GitBackend,
    lanes: Vec<Lane>,
    main_oid: &str,
    order: Order,
    observer: &dyn AnalysisObserver,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<LaneGroup>> {
    let _perf = AnalysisStage::new(observer, "groups");
    observer.progress(format_args!(
        "lane groups: building groups for {} lanes",
        lanes.len()
    ));
    let grouped = grouped_by_base(lanes);
    let bases: Vec<_> = grouped
        .iter()
        .filter_map(|(base, _)| base.clone())
        .collect();
    let metadata: Vec<_> = bases
        .iter()
        .filter(|base| base.as_str() != main_oid)
        .map(String::as_str)
        .collect();
    git.cache_commit_metas(&metadata, meta_cache)?;
    let distances = {
        let _perf = AnalysisStage::new(observer, "main_distances");
        git.main_distances(main_oid, &bases)?
    };
    let mut groups = Vec::new();
    for (base_oid, lanes) in grouped {
        let base_meta = match base_oid.as_deref() {
            Some(base_oid) if base_oid != main_oid => {
                Some(get_commit_meta(git, base_oid, meta_cache)?)
            }
            _ => None,
        };
        let main_distance = base_oid
            .as_ref()
            .and_then(|base| distances.get(base).copied().flatten());
        if let Some(base) = base_oid.as_deref() {
            observer.event(format_args!(
                "main_distance base={base} commits={}",
                main_distance.map_or_else(|| "unreachable".to_string(), |n| n.to_string())
            ));
        }

        groups.push(LaneGroup::new(base_oid, base_meta, main_distance, lanes));
    }

    observer.count("groups", groups.len());
    observer.count(
        "historical_bases",
        groups
            .iter()
            .filter(|g| g.main_distance.is_some_and(|n| n > 0))
            .count(),
    );
    observer.count(
        "orphaned_groups",
        groups.iter().filter(|g| g.main_distance.is_none()).count(),
    );
    sort_lane_groups(&mut groups, order);
    Ok(groups)
}

pub(crate) fn sort_lane_groups(groups: &mut [LaneGroup], order: Order) {
    groups.sort_by_cached_key(|group| lane_group_key(group, order));
}

/// Reference endpoints remain visible even without an attached feature lane.
pub(crate) fn include_mainline_groups(
    git: &dyn GitBackend,
    groups: &mut Vec<LaneGroup>,
    main_oid: &str,
    repository: &RepositorySnapshot,
    order: Order,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let mut ids: Vec<_> = repository
        .main_refs
        .iter()
        .map(|reference| reference.oid.clone())
        .filter(|oid| oid != main_oid)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    ids.sort();
    ids.dedup();
    let distances = git.main_distances(main_oid, &ids)?;
    for oid in ids {
        if groups
            .iter()
            .any(|group| group.base_oid.as_ref() == Some(&oid))
        {
            continue;
        }
        if let Some(distance) = distances.get(&oid).copied().flatten() {
            let meta = get_commit_meta(git, &oid, meta_cache)?;
            groups.push(LaneGroup::new(
                Some(oid),
                Some(meta),
                Some(distance),
                Vec::new(),
            ));
        }
    }
    sort_lane_groups(groups, order);
    Ok(())
}

pub(super) fn lane_group_key(group: &LaneGroup, order: Order) -> (bool, usize, i128, String) {
    let timestamp = i128::from(lane_group_timestamp(&group.lanes, order));
    (
        group.main_distance.is_none(),
        group.main_distance.unwrap_or(0),
        if order == Order::Newest {
            -timestamp
        } else {
            timestamp
        },
        group.base_oid.clone().unwrap_or_default(),
    )
}

pub(super) fn lane_group_timestamp(lanes: &[Lane], order: Order) -> i64 {
    match order {
        Order::Newest => lanes
            .iter()
            .map(|lane| lane.head_timestamp)
            .max()
            .unwrap_or(i64::MIN),
        Order::Oldest => lanes
            .iter()
            .map(|lane| lane.head_timestamp)
            .min()
            .unwrap_or(i64::MAX),
    }
}

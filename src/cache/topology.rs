//! Bounded, shared path representation for topology snapshots.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::{BranchPoint, Lane, LaneGroup, RewrittenCommit};

#[derive(Serialize, Deserialize)]
struct Topology {
    points: Vec<(BranchPoint, Option<usize>)>,
    groups: Vec<Group>,
}

#[derive(Serialize, Deserialize)]
struct Group {
    base: Option<String>,
    distance: Option<usize>,
    lanes: Vec<Path>,
}

#[derive(Serialize, Deserialize)]
struct Path {
    head: String,
    base: Option<String>,
    tip: Option<usize>,
    rewritten: Vec<RewrittenCommit>,
    timestamp: i64,
}

pub(crate) fn serialize<S: Serializer>(
    groups: &[LaneGroup],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut points = Vec::new();
    let mut identities = HashMap::new();
    let groups = groups
        .iter()
        .map(|group| Group {
            base: group.base_oid.clone(),
            distance: group.main_distance,
            lanes: group
                .lanes
                .iter()
                .map(|lane| {
                    let mut parent = None;
                    for point in lane.branch_points.iter().rev() {
                        let key = (parent, point.oid.as_str(), &point.names, point.commit_count);
                        let next = points.len();
                        let index = *identities.entry(key).or_insert_with(|| {
                            let mut point = point.clone();
                            point.annotation = None;
                            point.diff_stat = None;
                            points.push((point, parent));
                            next
                        });
                        parent = Some(index);
                    }
                    Path {
                        head: lane.head_oid.clone(),
                        base: lane.base_oid.clone(),
                        tip: parent,
                        rewritten: lane.rewritten_commits.clone(),
                        timestamp: lane.head_timestamp,
                    }
                })
                .collect(),
        })
        .collect();
    crate::diagnostics::count("snapshot_shared_points", points.len());
    Topology { points, groups }.serialize(serializer)
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<LaneGroup>, D::Error> {
    let topology = Topology::deserialize(deserializer)?;
    topology
        .groups
        .into_iter()
        .map(|group| {
            let lanes = group
                .lanes
                .into_iter()
                .map(|lane| {
                    let mut branch_points = Vec::new();
                    let mut cursor = lane.tip;
                    while let Some(index) = cursor {
                        let (point, parent) = topology
                            .points
                            .get(index)
                            .ok_or_else(|| serde::de::Error::custom("invalid snapshot point"))?;
                        if parent.is_some_and(|parent| parent >= index) {
                            return Err(serde::de::Error::custom("cyclic snapshot path"));
                        }
                        branch_points.push(point.clone());
                        cursor = *parent;
                    }
                    Ok(Lane::new_with_rewritten_commits(
                        lane.head,
                        lane.base,
                        branch_points,
                        lane.rewritten,
                        lane.timestamp,
                        false,
                    ))
                })
                .collect::<Result<_, D::Error>>()?;
            Ok(LaneGroup::new(group.base, None, group.distance, lanes))
        })
        .collect()
}

#[cfg(test)]
mod tests;

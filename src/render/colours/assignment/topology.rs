//! Extract segment weights and visual adjacency without terminal or repository operations.

use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

use crate::model::LaneGroup;
use crate::render::ancestry::{Content, PreparedTopology, shared_ancestry};

use super::Segment;

#[derive(Default)]
struct Graph {
    segments: Vec<Segment>,
    edges: Vec<(usize, usize)>,
    previous: Option<usize>,
}

pub(super) fn segments(groups: &[LaneGroup]) -> Vec<Segment> {
    segments_prepared(groups, None)
}

pub(super) fn segments_prepared(
    groups: &[LaneGroup],
    prepared: Option<&PreparedTopology>,
) -> Vec<Segment> {
    let mut graph = Graph::default();
    for (index, group) in groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.main_distance.is_some())
    {
        let mut active = BTreeMap::new();
        let ancestry = prepared
            .and_then(|prepared| prepared.groups.get(index))
            .map_or_else(
                || shared_ancestry(&group.lanes),
                |prepared| prepared.bind(&group.lanes),
            );
        for row in ancestry.rows {
            let segment = active
                .get(&row.lane)
                .copied()
                .filter(|_| row.children.len() <= 1)
                .unwrap_or_else(|| graph.start(&mut active, row.lane, &row.children));
            if let Some(previous) = graph.previous {
                graph.connect(previous, segment);
            }
            graph.previous = Some(segment);
            let node = &mut graph.segments[segment];
            node.weight += 1;
            if let Content::Branch(point) = row.content {
                for name in &point.names {
                    if node.identity.is_empty() || name < &node.identity {
                        node.identity.clone_from(name);
                    }
                }
            }
        }
    }
    graph.edges.sort_unstable();
    graph.edges.dedup();
    for (left, right) in graph.edges {
        graph.segments[left].neighbours.push(right);
        graph.segments[right].neighbours.push(left);
    }
    graph.segments
}

impl Graph {
    fn start(
        &mut self,
        active: &mut BTreeMap<usize, usize>,
        lane: usize,
        children: &[usize],
    ) -> usize {
        let children = if children.len() > 1 { children } else { &[] };
        let retired: Vec<_> = children
            .iter()
            .filter_map(|child| active.remove(child))
            .collect();
        let segment = self.segments.len();
        self.segments.push(Segment {
            weight: 0,
            identity: String::new(),
            neighbours: Vec::new(),
        });
        active.insert(lane, segment);
        self.connect_neighbours(active, lane);
        for &child in children {
            self.connect_neighbours(active, child);
        }
        for child in retired {
            self.connect(child, segment);
        }
        segment
    }

    fn connect_neighbours(&mut self, active: &BTreeMap<usize, usize>, lane: usize) {
        let left = active.range(..lane).next_back().map(|(_, &value)| value);
        let right = active
            .range((Excluded(lane), Unbounded))
            .next()
            .map(|(_, &value)| value);
        if let Some(&centre) = active.get(&lane) {
            for neighbour in left.into_iter().chain(right) {
                self.connect(centre, neighbour);
            }
        } else if let (Some(left), Some(right)) = (left, right) {
            self.connect(left, right);
        }
    }

    fn connect(&mut self, left: usize, right: usize) {
        if left != right {
            self.edges.push((left.min(right), left.max(right)));
        }
    }
}

#[cfg(test)]
#[path = "../tests/topology.rs"]
mod tests;

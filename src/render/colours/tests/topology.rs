use std::collections::BTreeSet;

use super::*;

#[test]
fn joining_interleaved_lanes_connects_new_neighbours_without_transient_edges() {
    let mut graph = Graph::default();
    let mut active = BTreeMap::new();
    let left = graph.start(&mut active, 0, &[]);
    let child_left = graph.start(&mut active, 1, &[]);
    let middle = graph.start(&mut active, 2, &[]);
    let child_right = graph.start(&mut active, 3, &[]);
    let right = graph.start(&mut active, 4, &[]);
    graph.edges.clear();

    let parent = graph.start(&mut active, 1, &[1, 3]);

    let edges: BTreeSet<_> = graph.edges.into_iter().collect();
    assert_eq!(
        edges,
        BTreeSet::from([
            (left, parent),
            (child_left, parent),
            (middle, parent),
            (child_right, parent),
            (middle, right),
        ])
    );
    assert_eq!(
        active,
        BTreeMap::from([(0, left), (1, parent), (2, middle), (4, right)])
    );
}

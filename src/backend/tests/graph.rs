use super::*;

fn ancestors(nodes: &[(usize, Vec<usize>)], head: usize) -> Vec<bool> {
    let mut seen = vec![false; nodes.len()];
    let mut stack = vec![head];
    while let Some(id) = stack.pop() {
        if !seen[id] {
            seen[id] = true;
            stack.extend(&nodes[id].1);
        }
    }
    seen
}

#[test]
fn classifies_every_five_vertex_dag_against_independent_ancestor_searches() {
    // Enumerate every DAG whose edges respect this topological labelling.
    // Reverse the candidate order to ensure classification is not date-dependent.
    for mask in 0..1024_u16 {
        let mut bit = 0;
        let nodes: Vec<_> = (0..5)
            .map(|id| {
                let parents = (0..id)
                    .filter(|_| {
                        let present = mask & (1 << bit) != 0;
                        bit += 1;
                        present
                    })
                    .collect();
                (id, parents)
            })
            .collect();
        for main in 0..5 {
            let hidden = ancestors(&nodes, main);
            let range: Vec<_> = nodes
                .iter()
                .filter(|(id, _)| !hidden[*id])
                .cloned()
                .rev()
                .collect();
            let actual = proven_bases(&range);
            for (head, _) in &range {
                let reachable = ancestors(&nodes, *head);
                let outside: std::collections::HashSet<_> = range
                    .iter()
                    .filter(|(id, _)| reachable[*id])
                    .flat_map(|(_, parents)| parents)
                    .filter(|id| hidden[**id])
                    .copied()
                    .collect();
                let expected = match outside.len() {
                    0 => Some(None),
                    1 => Some(outside.into_iter().next()),
                    _ => None,
                };
                assert_eq!(
                    actual.get(head).copied(),
                    expected,
                    "mask={mask} main={main} head={head}"
                );
            }
        }
        for head in 0..5 {
            for base in 0..5 {
                let reachable = ancestors(&nodes, head);
                let hidden = ancestors(&nodes, base);
                let candidates: Vec<_> = nodes
                    .iter()
                    .filter(|(id, _)| reachable[*id] && !hidden[*id])
                    .cloned()
                    .rev()
                    .collect();
                let actual = descendants(&candidates, base);
                assert_eq!(
                    descendant_counts(&candidates, &[base])[0],
                    actual.iter().filter(|included| **included).count()
                );
                for ((id, _), actual) in candidates.iter().zip(actual) {
                    assert_eq!(
                        actual,
                        ancestors(&nodes, *id)[base],
                        "mask={mask} head={head} base={base} id={id}"
                    );
                }
            }
        }
    }
}

#[test]
fn distance_masks_span_blocks_and_do_not_double_count_merges() {
    let nodes = vec![(3, vec![2, 1]), (2, vec![0]), (1, vec![0])];
    assert_eq!(descendant_counts(&nodes, &vec![0; 130]), vec![3; 130]);
    assert_eq!(
        descendant_counts(&nodes, &[0, 1, 2, 3, 99]),
        [3, 1, 1, 0, 0]
    );
}

#[test]
fn redundant_parents_and_hidden_history_are_handled_without_revisiting_boundaries() {
    let nodes = vec![(3, vec![2, 1]), (2, vec![1]), (1, vec![0]), (4, vec![99])];
    assert_eq!(descendants(&nodes, 0), [true, true, true, false]);
    assert_eq!(descendants::<usize>(&[], 0), Vec::<bool>::new());
}

#[test]
fn summarises_shared_linear_corridors_and_rejects_ambiguous_graphs() {
    use std::collections::HashSet;
    let nodes = vec![(5, vec![3]), (4, vec![3]), (3, vec![2]), (2, vec![1])];
    let paths = crate::backend::graph::linear_paths(&nodes, &HashSet::from([2, 4, 5])).unwrap();
    assert_eq!(paths.summary(&4), Some((Some(1), vec![(4, 2), (2, 1)])));
    assert_eq!(paths.summary(&5), Some((Some(1), vec![(5, 2), (2, 1)])));
    assert!(paths.summary(&99).is_none());
    assert!(
        crate::backend::graph::linear_paths(&[(1, vec![2]), (2, vec![1])], &HashSet::new())
            .is_none()
    );
    assert!(crate::backend::graph::linear_paths(&[(3, vec![1, 2])], &HashSet::new()).is_none());
    let orphan = crate::backend::graph::linear_paths(&[(1, vec![])], &HashSet::from([1])).unwrap();
    assert_eq!(orphan.summary(&1), Some((None, vec![(1, 1)])));
}

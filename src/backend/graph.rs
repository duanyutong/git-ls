use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Classifies a revision range using only edges already returned by its walk.
/// Parents outside the range are boundaries; only `base` seeds reachability.
pub(super) fn descendants<K: Copy + Eq + Hash>(nodes: &[(K, Vec<K>)], base: K) -> Vec<bool> {
    let index: HashMap<K, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let mut children = vec![Vec::new(); nodes.len()];
    let mut included = vec![false; nodes.len()];
    let mut pending = VecDeque::new();
    for (child, (_, parents)) in nodes.iter().enumerate() {
        for parent in parents {
            if *parent == base && !included[child] {
                included[child] = true;
                pending.push_back(child);
            }
            if let Some(parent) = index.get(parent) {
                children[*parent].push(child);
            }
        }
    }
    while let Some(parent) = pending.pop_front() {
        for &child in &children[parent] {
            if !included[child] {
                included[child] = true;
                pending.push_back(child);
            }
        }
    }
    included
}

#[cfg(test)]
mod tests {
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
    fn redundant_parents_and_hidden_history_are_handled_without_revisiting_boundaries() {
        let nodes = vec![(3, vec![2, 1]), (2, vec![1]), (1, vec![0]), (4, vec![99])];
        assert_eq!(descendants(&nodes, 0), [true, true, true, false]);
        assert_eq!(descendants::<usize>(&[], 0), Vec::<bool>::new());
    }
}

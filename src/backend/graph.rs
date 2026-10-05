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

struct IndexedGraph<K> {
    index: HashMap<K, usize>,
    children: Vec<Vec<usize>>,
    order: Vec<usize>,
}

fn indexed_graph<K: Copy + Eq + Hash>(nodes: &[(K, Vec<K>)]) -> IndexedGraph<K> {
    let index: HashMap<K, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let mut children = vec![Vec::new(); nodes.len()];
    let mut remaining = vec![0; nodes.len()];
    for (child, (_, parents)) in nodes.iter().enumerate() {
        for parent in parents {
            if let Some(parent) = index.get(parent) {
                children[*parent].push(child);
                remaining[child] += 1;
            }
        }
    }
    let mut ready: VecDeque<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(parent) = ready.pop_front() {
        order.push(parent);
        for &child in &children[parent] {
            remaining[child] -= 1;
            if remaining[child] == 0 {
                ready.push_back(child);
            }
        }
    }
    IndexedGraph {
        index,
        children,
        order,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary<K> {
    Empty,
    One(K),
    Multiple,
}

impl<K: Copy + Eq> Boundary<K> {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Empty, value) | (value, Self::Empty) => value,
            (Self::One(a), Self::One(b)) if a == b => Self::One(a),
            _ => Self::Multiple,
        }
    }
}

/// A complete tip-minus-main range proves a merge base when exactly one
/// outside parent is reachable. Several boundaries require the backend's
/// usual merge-base policy; no boundary proves an unrelated history.
pub(super) fn proven_bases<K: Copy + Eq + Hash>(nodes: &[(K, Vec<K>)]) -> HashMap<K, Option<K>> {
    let graph = indexed_graph(nodes);
    let mut boundaries = vec![Boundary::Empty; nodes.len()];
    let mut bases = HashMap::new();
    for vertex in graph.order {
        for parent in &nodes[vertex].1 {
            let boundary = graph
                .index
                .get(parent)
                .map_or(Boundary::One(*parent), |i| boundaries[*i]);
            boundaries[vertex] = boundaries[vertex].join(boundary);
        }
        match boundaries[vertex] {
            Boundary::Empty => {
                bases.insert(nodes[vertex].0, None);
            }
            Boundary::One(base) => {
                bases.insert(nodes[vertex].0, Some(base));
            }
            Boundary::Multiple => {}
        }
    }
    bases
}

/// Counts strict descendants for many bases in a shared graph. A block of 64
/// bases requires one mask per vertex, independently of the total base count.
pub(super) fn descendant_counts<K: Copy + Eq + Hash>(
    nodes: &[(K, Vec<K>)],
    bases: &[K],
) -> Vec<usize> {
    let IndexedGraph {
        index,
        children,
        order,
    } = indexed_graph(nodes);
    let mut counts = vec![0; bases.len()];
    for (block, bases) in bases.chunks(64).enumerate() {
        let seeds: HashMap<K, u64> =
            bases
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut seeds, (i, base)| {
                    *seeds.entry(*base).or_default() |= 1_u64 << i;
                    seeds
                });
        let mut masks = vec![0_u64; nodes.len()];
        for &vertex in &order {
            let (id, parents) = &nodes[vertex];
            for parent in parents {
                if !index.contains_key(parent) {
                    masks[vertex] |= seeds.get(parent).copied().unwrap_or(0);
                }
            }
            let mut bits = masks[vertex];
            while bits != 0 {
                counts[block * 64 + bits.trailing_zeros() as usize] += 1;
                bits &= bits - 1;
            }
            masks[vertex] |= seeds.get(id).copied().unwrap_or(0);
            for &child in &children[vertex] {
                masks[child] |= masks[vertex];
            }
        }
    }
    counts
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
}

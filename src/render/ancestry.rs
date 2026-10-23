use std::collections::HashMap;

use crate::model::{BranchPoint, Lane, RewrittenCommit};

#[derive(Clone, Copy)]
pub(super) enum Content<'a> {
    Branch(&'a BranchPoint),
    Rewritten(&'a RewrittenCommit),
}

impl<'a> Content<'a> {
    fn identity(self) -> (&'a str, Option<&'a str>) {
        match self {
            Self::Branch(point) => (&point.oid, None),
            Self::Rewritten(commit) => (&commit.meta.oid, Some(&commit.replacement.oid)),
        }
    }
}

pub(super) struct AncestryRow<'a> {
    pub(super) content: Content<'a>,
    pub(super) lane: usize,
    pub(super) children: Vec<usize>,
}

pub(super) struct Ancestry<'a> {
    pub(super) rows: Vec<AncestryRow<'a>>,
    pub(super) roots: Vec<usize>,
}

struct Node<'a> {
    content: Content<'a>,
    lane: usize,
    children: Vec<usize>,
}

/// Share identical downstack paths.
/// Distinct paths through merged history remain separate even when commit identifiers coincide.
pub(super) fn shared_ancestry(lanes: &[Lane]) -> Ancestry<'_> {
    let mut nodes: Vec<Node<'_>> = Vec::new();
    let mut identities = HashMap::new();
    let mut roots = Vec::new();
    for (lane, path) in lanes.iter().enumerate() {
        let mut parent: Option<usize> = None;
        let contents = path
            .rewritten_commits
            .iter()
            .rev()
            .map(Content::Rewritten)
            .chain(path.branch_points.iter().rev().map(Content::Branch));
        for content in contents {
            let key = (parent, content.identity());
            let index = *identities.entry(key).or_insert_with(|| {
                let index = nodes.len();
                nodes.push(Node {
                    content,
                    lane,
                    children: Vec::new(),
                });
                if let Some(parent) = parent {
                    nodes[parent].children.push(index);
                } else {
                    roots.push(index);
                }
                index
            });
            parent = Some(index);
        }
    }

    let root_lanes = roots.iter().map(|&index| nodes[index].lane).collect();
    let mut pending: Vec<_> = roots
        .into_iter()
        .rev()
        .map(|index| (index, false))
        .collect();
    let mut rows = Vec::with_capacity(nodes.len());
    while let Some((index, visited)) = pending.pop() {
        let node = &nodes[index];
        if visited {
            rows.push(AncestryRow {
                content: node.content,
                lane: node.lane,
                children: node
                    .children
                    .iter()
                    .map(|&child| nodes[child].lane)
                    .collect(),
            });
        } else {
            pending.push((index, true));
            pending.extend(node.children.iter().rev().map(|&child| (child, false)));
        }
    }
    Ancestry {
        rows,
        roots: root_lanes,
    }
}

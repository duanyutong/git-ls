use super::*;
use crate::backend::{DiffBatch, DiffPair, DiffStatBackend};
use crate::model::{BranchPoint, DiffStat, Lane};
use std::cell::RefCell;

fn lane(base: Option<&str>, points: &[&str]) -> Lane {
    Lane::new(
        "head",
        base.map(str::to_string),
        points
            .iter()
            .map(|oid| BranchPoint::new(*oid, [*oid], None))
            .collect(),
        0,
        false,
    )
}

#[derive(Default)]
struct Statistics {
    calls: RefCell<Vec<Vec<DiffPair>>>,
    wrong_count: bool,
    fail: bool,
}

impl DiffStatBackend for Statistics {
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        self.calls.borrow_mut().push(pairs.to_vec());
        if self.fail {
            return Err(GitLsError::TestFixture("diff failed".into()));
        }
        Ok(DiffBatch {
            stats: if self.wrong_count {
                Vec::new()
            } else {
                pairs
                    .iter()
                    .map(|pair| match pair.head.as_str() {
                        "base" => DiffStat {
                            added: 10,
                            deleted: 1,
                        },
                        "left" => DiffStat {
                            added: 2,
                            deleted: 0,
                        },
                        "right" => DiffStat {
                            added: 0,
                            deleted: 3,
                        },
                        _ => DiffStat {
                            added: 4,
                            deleted: 0,
                        },
                    })
                    .collect()
            },
            paths: Vec::new(),
        })
    }
    fn diff_context(&self, _paths: &[String]) -> Option<u64> {
        None
    }
}

#[test]
fn calculates_each_layer_once_across_shared_lanes_and_handles_orphans() {
    let git = Statistics::default();
    let mut groups = vec![
        LaneGroup::new(
            Some("main".into()),
            None,
            Some(0),
            vec![
                lane(Some("main"), &["left", "base"]),
                lane(Some("main"), &["right", "base"]),
            ],
        ),
        LaneGroup::new(None, None, None, vec![lane(None, &["island"])]),
    ];
    populate_diff_stats(&mut groups, &git, false).unwrap();
    assert!(git.calls.borrow().is_empty());
    populate_diff_stats(&mut groups, &git, true).unwrap();
    assert_eq!(
        groups[0].lanes[0].branch_points[0].diff_stat,
        Some(DiffStat {
            added: 2,
            deleted: 0
        })
    );
    assert_eq!(
        groups[0].lanes[1].branch_points[1].diff_stat,
        Some(DiffStat {
            added: 10,
            deleted: 1
        })
    );
    assert_eq!(
        groups[1].lanes[0].branch_points[0].diff_stat,
        Some(DiffStat {
            added: 4,
            deleted: 0
        })
    );
    assert_eq!(git.calls.borrow().len(), 1);
    assert_eq!(git.calls.borrow()[0].len(), 4);
    assert_eq!(git.calls.borrow()[0][0].base.as_deref(), Some("main"));
    assert_eq!(git.calls.borrow()[0][3].base, None);
    populate_diff_stats(&mut [], &git, true).unwrap();
    assert!(
        populate_diff_stats(
            &mut groups,
            &Statistics {
                wrong_count: true,
                ..Statistics::default()
            },
            true
        )
        .is_err()
    );
    assert!(
        populate_diff_stats(
            &mut groups,
            &Statistics {
                fail: true,
                ..Statistics::default()
            },
            true
        )
        .is_err()
    );
}

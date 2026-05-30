mod colours;
mod columns;
mod graph;
mod layout;
mod line;
mod metadata;
mod orphan;
mod rewrite;
mod trunk;

use super::Colours;

use crate::cli::Palette;
use crate::model::{BranchAnnotation, BranchPoint, CommitMeta};
use crate::test_support::TEST_COMMIT_TIME;

fn point(oid: &str, names: &[&str]) -> BranchPoint {
    BranchPoint::new(oid, names.iter().copied(), None)
}

fn point_with_count(oid: &str, names: &[&str], commit_count: usize, subject: &str) -> BranchPoint {
    point_with_count_at(oid, names, commit_count, subject, TEST_COMMIT_TIME)
}

fn point_with_count_at(
    oid: &str,
    names: &[&str],
    commit_count: usize,
    subject: &str,
    timestamp: i64,
) -> BranchPoint {
    BranchPoint::new(
        oid,
        names.iter().copied(),
        Some(BranchAnnotation::new(
            CommitMeta::new(oid, timestamp, subject),
            commit_count,
        )),
    )
}

fn meta(oid: &str, subject: &str) -> CommitMeta {
    CommitMeta::new(oid, 0, subject)
}

fn test_colours(enabled: bool) -> Colours {
    Colours::new(enabled, Palette::Classic)
}

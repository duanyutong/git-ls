use crate::cli::{Layout, Verbosity};
use crate::model::{CommitMeta, RewrittenCommit};
use crate::render::RenderContext;
use crate::render::metadata::MetadataWidths;
use crate::render::rewrite::display_rewritten_commit;
use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

use super::test_colours;

#[test]
fn renders_rewritten_commit_metadata_and_title() {
    let colours = test_colours(false);
    let commit = RewrittenCommit::new(
        CommitMeta::new("old-oid", TEST_COMMIT_TIME, "old subject"),
        CommitMeta::new("new-oid", TEST_NOW, "new subject"),
    );
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths { age: 2, count: 1 },
        &colours,
    )
    .with_layout(Layout::Inline);

    assert_eq!(
        display_rewritten_commit(&commit, &ctx),
        "2m (rewritten as new-oid) old-oid old subject"
    );
}

#[test]
fn renders_rewritten_commit_summary_without_identifiers() {
    let colours = test_colours(false);
    let commit = RewrittenCommit::new(
        CommitMeta::new("old-oid", TEST_COMMIT_TIME, "old subject"),
        CommitMeta::new("new-oid", TEST_NOW, "new subject"),
    );
    let ctx = RenderContext::new(
        "main",
        None,
        None,
        None,
        TEST_NOW,
        Verbosity::Medium,
        MetadataWidths { age: 2, count: 1 },
        &colours,
    )
    .with_layout(Layout::Inline);

    assert_eq!(display_rewritten_commit(&commit, &ctx), "2m (rewritten)");
}

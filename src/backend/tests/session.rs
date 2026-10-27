use super::*;
use crate::test_support::MockGit;

#[test]
fn memoises_unchanged_empty_and_ambiguous_rewrites() {
    for result in ["oid", "", "a\nb"] {
        let git = MockGit::default().with(
            &["branchless", "query", "-r", "--hidden", "current(oid)"],
            result,
        );
        let session = Session::new(&git);
        let first = session.query_revset("current(oid)", true).unwrap();
        assert_eq!(first, session.query_revset("current(oid)", true).unwrap());
        assert_eq!(git.calls().len(), 1);
        assert!(!session.has_new_facts());
    }
}

#[test]
fn rewrite_failures_are_preserved_and_full_memo_tables_keep_live_results() {
    let missing = MockGit::default();
    let session = Session::new(&missing);
    assert!(
        session
            .query_rewrites(&["old".into()], &["target".into()])
            .is_err()
    );
    assert!(
        session
            .query_rewrites_in_path(None, "tip", &["old".into()], &["target".into()])
            .is_err()
    );
    let git = MockGit::default().with(
        &["branchless", "query", "-r", "--hidden", "current(old)"],
        "target",
    );
    let mut session = Session::new(&git);
    session.limit = 0;
    for _ in 0..2 {
        assert_eq!(
            session
                .query_rewrites(&["old".into()], &["target".into()])
                .unwrap()["old"],
            "target"
        );
    }
    assert_eq!(git.calls().len(), 2);
}

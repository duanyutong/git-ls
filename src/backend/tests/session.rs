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

use super::*;

#[test]
fn assembly_rejects_a_missing_comparison_instead_of_inventing_zero_counts() {
    let pair = DiffPair {
        base: None,
        head: "head".into(),
    };
    assert!(assembled(&[pair], &HashMap::new(), Vec::new()).is_err());
}

#[test]
fn records_without_dependency_proofs_are_neither_reused_nor_admitted() {
    let git = crate::test_support::MockGit::default();
    let session = Session::new(&git);
    let pair = DiffPair {
        base: None,
        head: "head".into(),
    };
    let context = DiffContext::uniform(1);
    let batch = DiffBatch {
        stats: vec![DiffStat::default()],
        paths: Vec::new(),
    };
    session.retain_statistics(
        std::slice::from_ref(&pair),
        &batch,
        &HashMap::new(),
        &context,
    );
    assert!(session.facts.borrow().diff.scopes[&0].values.is_empty());
    session
        .facts
        .borrow_mut()
        .diff
        .scopes
        .get_mut(&0)
        .unwrap()
        .values
        .insert(
            pair.key(),
            DiffValue {
                stat: DiffStat::default(),
                paths: vec!["missing".into()],
                fingerprint: 1,
                used: 0,
            },
        );
    assert!(
        session
            .cached_statistics(&[pair], &HashMap::new(), &context)
            .is_empty()
    );
}

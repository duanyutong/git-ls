use super::*;

#[test]
fn answer_comparison_ignores_recency_but_detects_each_fact_category() {
    let original = Facts::default();
    let mut facts = original.clone();
    facts.touch("unused");
    assert!(facts.same_answers(&original));
    facts.bases.insert("base".into(), None);
    assert!(!facts.same_answers(&original));
    facts.bases.clear();
    facts.distances.insert("distance".into(), None);
    assert!(!facts.same_answers(&original));
    facts.distances.clear();
    facts
        .metadata
        .insert("meta".into(), CommitMeta::new("meta", 0, "subject"));
    assert!(!facts.same_answers(&original));
    facts.metadata.clear();
    facts.diff.scopes.insert(1, DiffScope::default());
    assert!(!facts.same_answers(&original));
}

#[test]
fn statistics_have_bounded_retention_and_are_discarded_before_graph_facts() {
    let mut facts = Facts::default();
    facts.bases.insert("base".into(), None);
    for (used, key) in [(1, "old"), (2, "new")] {
        facts.diff.scopes.entry(1).or_default().values.insert(
            key.into(),
            DiffValue {
                stat: DiffStat::default(),
                paths: vec!["file".into()],
                fingerprint: 1,
                used,
            },
        );
    }
    facts.compact(1);
    assert_eq!(
        facts.diff.scopes[&1].values.keys().collect::<Vec<_>>(),
        vec!["new"]
    );
    facts.fit(1, &|value| value.diff.scopes.is_empty());
    assert_eq!(facts.diff, DiffFacts::default());
    assert!(facts.bases.contains_key("base"));
}
#[test]
fn compaction_retains_recently_reused_facts() {
    let mut facts = Facts::default();
    for key in ["old", "warm", "new"] {
        facts.bases.insert(key.to_string(), None);
        facts.distances.insert(key.to_string(), Some(1));
        facts
            .metadata
            .insert(key.to_string(), CommitMeta::new(key, 0, key));
        facts.touch(key);
    }
    facts.touch("warm");
    facts.recency.insert("discarded".to_string(), 100);
    facts.compact(1);
    assert_eq!(facts.bases.keys().collect::<Vec<_>>(), vec!["warm"]);
    assert_eq!(facts.distances.len(), 1);
    assert_eq!(facts.metadata.len(), 1);
    assert_eq!(facts.recency.len(), 1);
    let mut facts = Facts::default();
    facts.bases.insert("base".to_string(), None);
    facts.bases.insert("untouched".to_string(), None);
    facts.distances.insert("distance".to_string(), None);
    facts.metadata.insert(
        "metadata".to_string(),
        CommitMeta::new("metadata", 0, "meta"),
    );
    for key in ["base", "distance", "metadata", "removed"] {
        facts.touch(key);
    }
    facts.compact(1);
    assert_eq!(facts.recency.len(), 3);
}

#[test]
fn byte_admission_prefers_graph_facts_and_recent_metadata() {
    let mut facts = Facts::default();
    for key in ["old", "warm", "new"] {
        facts
            .metadata
            .insert(key.into(), CommitMeta::new(key, 0, key));
        facts.touch(key);
    }
    facts.touch("warm");
    facts.bases.insert("base".into(), Some("oid".into()));
    facts.bases.insert("other-base".into(), None);
    facts.distances.insert("distance".into(), Some(1));
    facts.distances.insert("other-distance".into(), None);
    facts.fit(512, &|value| value.metadata.len() <= 1);
    assert!(facts.metadata.contains_key("warm"));
    assert_eq!(facts.bases.len(), 2);
    assert_eq!(facts.distances.len(), 2);
    assert_eq!(facts.recency.len(), 1);
    facts.fit(512, &|value| {
        value.metadata.is_empty() && value.bases.is_empty() && value.distances.is_empty()
    });
    assert!(facts.metadata.is_empty());
    assert!(facts.bases.is_empty());
    assert!(facts.distances.is_empty());
    facts.fit(0, &|_| false);
    assert!(facts.recency.is_empty());
}

#[test]
fn worktree_contexts_and_tree_mappings_share_a_bounded_recent_working_set() {
    let mut facts = Facts::default();
    for scope in 0..6 {
        let used = facts.diff.tick();
        facts.diff.scopes.insert(
            scope,
            DiffScope {
                used,
                values: HashMap::from([(
                    format!("tree-{scope}"),
                    DiffValue {
                        stat: DiffStat::default(),
                        paths: vec![format!("path-{scope}")],
                        fingerprint: scope,
                        used,
                    },
                )]),
            },
        );
        facts
            .diff
            .trees
            .insert(format!("commit-{scope}"), format!("tree-{scope}"));
        facts.touch(&format!("commit-{scope}"));
    }
    let mut touched = facts.clone();
    touched.diff.clock += 100;
    touched.diff.scopes.get_mut(&5).unwrap().used = 100;
    touched
        .diff
        .scopes
        .get_mut(&5)
        .unwrap()
        .values
        .get_mut("tree-5")
        .unwrap()
        .used = 100;
    assert!(facts.same_answers(&touched));
    touched
        .diff
        .scopes
        .get_mut(&5)
        .unwrap()
        .values
        .get_mut("tree-5")
        .unwrap()
        .fingerprint = 99;
    assert!(!facts.same_answers(&touched));
    facts.compact(3);
    assert_eq!(facts.diff.trees.len(), 3);
    assert_eq!(
        facts
            .diff
            .scopes
            .values()
            .map(|scope| scope.values.len())
            .sum::<usize>(),
        3
    );
    assert!(!facts.diff.scopes.contains_key(&0));
    let mut changed = facts.clone();
    changed.diff.trees.insert("new".into(), "tree".into());
    assert!(!changed.same_answers(&facts));
    facts.fit(3, &|value| value.diff.scopes.len() <= 1);
    assert_eq!(facts.diff.scopes.len(), 1);
    assert!(facts.diff.scopes.contains_key(&5));
}

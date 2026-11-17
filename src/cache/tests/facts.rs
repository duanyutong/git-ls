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
                paths: std::rc::Rc::new(vec!["file".into()]),
                fingerprint: 1,
                configuration: 1,
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
                        paths: std::rc::Rc::new(vec![format!("path-{scope}")]),
                        fingerprint: scope,
                        configuration: scope,
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
    let mut configuration = facts.clone();
    configuration
        .diff
        .scopes
        .get_mut(&5)
        .unwrap()
        .values
        .get_mut("tree-5")
        .unwrap()
        .configuration = 99;
    assert!(!facts.same_answers(&configuration));
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

fn statistics_fixture(count: usize) -> Facts {
    let mut facts = Facts::default();
    for index in 0..count {
        let key = format!("pair-{index:04}");
        facts.diff.scopes.entry(1).or_default().values.insert(
            key,
            DiffValue {
                stat: DiffStat {
                    added: index as u64,
                    deleted: 0,
                },
                paths: std::rc::Rc::new(vec!["x".repeat(500)]),
                fingerprint: 1,
                configuration: 1,
                used: index as u64,
            },
        );
    }
    facts
}

#[test]
fn byte_admission_preserves_requested_comparisons_and_tree_mappings() {
    let mut facts = statistics_fixture(20);
    facts.diff.trees.insert("commit".into(), "tree".into());
    facts
        .diff
        .active
        .extend([(1, "pair-0000".into()), (1, "pair-0001".into())]);
    let mut current = facts.clone();
    current
        .diff
        .scopes
        .get_mut(&1)
        .unwrap()
        .values
        .retain(|key, _| key == "pair-0000" || key == "pair-0001");
    let bound = serde_json::to_vec(&current).unwrap().len();
    facts.fit(4096, &|value| {
        serde_json::to_vec(value).unwrap().len() <= bound
    });
    assert_eq!(facts.diff.scopes[&1].values.len(), 2);
    assert!(facts.diff.scopes[&1].values.contains_key("pair-0000"));
    assert!(facts.diff.scopes[&1].values.contains_key("pair-0001"));
    assert_eq!(facts.diff.trees["commit"], "tree");
    assert!(serde_json::to_vec(&facts).unwrap().len() <= bound);
    let encoded = serde_json::to_value(&facts).unwrap();
    assert!(encoded["diff"].get("active").is_none());
    let mut transient = facts.clone();
    transient.diff.active.clear();
    assert!(facts.same_answers(&transient));
}

#[test]
fn admission_uses_logarithmically_many_envelope_checks_and_bounded_partial_retention() {
    let mut facts = statistics_fixture(128);
    let full = serde_json::to_vec(&facts).unwrap().len();
    facts.diff.active = facts.diff.scopes[&1]
        .values
        .keys()
        .map(|key| (1, key.clone()))
        .collect();
    let calls = std::cell::Cell::new(0);
    facts.fit(4096, &|value| {
        calls.set(calls.get() + 1);
        serde_json::to_vec(value).unwrap().len() <= full / 2
    });
    let retained = facts.diff.scopes[&1].values.len();
    assert!(retained > 0 && retained < 128);
    assert_eq!(facts.diff.active.len(), retained);
    assert!(calls.get() <= 12, "{} envelope checks", calls.get());
    let mut count_limited = statistics_fixture(4);
    count_limited.diff.active.insert((1, "pair-0000".into()));
    count_limited.compact(1);
    assert!(
        count_limited.diff.scopes[&1]
            .values
            .contains_key("pair-0000")
    );
}

#[test]
fn oversized_tree_mappings_are_trimmed_without_dropping_graph_answers() {
    let mut facts = Facts::default();
    facts.bases.insert("base".into(), None);
    facts.diff.trees.insert("commit".into(), "tree".into());
    facts.fit(4096, &|value| value.diff.trees.is_empty());
    assert!(facts.diff.trees.is_empty());
    assert!(facts.bases.contains_key("base"));
}

#[test]
fn fitting_working_set_preserves_all_answers_and_active_pairs() {
    let mut facts = statistics_fixture(3);
    facts.diff.trees.insert("commit".into(), "tree".into());
    facts.diff.active.insert((1, "pair-0000".into()));
    let original = facts.clone();
    facts.fit(4096, &|_| true);
    assert!(facts.same_answers(&original));
    assert_eq!(facts.diff.active, original.diff.active);
}

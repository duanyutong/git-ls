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
    facts.diff.context = Some(1);
    assert!(!facts.same_answers(&original));
}

#[test]
fn statistics_have_bounded_retention_and_are_discarded_before_graph_facts() {
    let mut facts = Facts::default();
    facts.bases.insert("base".into(), None);
    facts.diff.context = Some(1);
    facts.diff.paths = vec!["file".into()];
    for key in ["old", "new"] {
        facts.diff.values.insert(key.into(), DiffStat::default());
        facts.touch(key);
    }
    facts.compact(1);
    assert_eq!(facts.diff.values.keys().collect::<Vec<_>>(), vec!["new"]);
    assert!(facts.recency.contains_key("new"));
    facts.fit(1, &|value| value.diff.values.is_empty());
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

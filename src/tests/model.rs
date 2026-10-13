use super::*;

#[test]
fn display_short_oid_uses_fixed_seven_character_width() {
    assert_eq!(
        display_short_oid("309567f69abcdef0123456789abcdef01234567"),
        "309567f"
    );
    assert_eq!(display_short_oid("abc123"), "abc123");
}

#[test]
fn commit_metadata_derives_display_oid() {
    let meta = CommitMeta::new("309567f69abcdef0123456789abcdef01234567", 123, "subject");

    assert_eq!(meta.short_oid, "309567f");
    assert_eq!(meta.timestamp, 123);
    assert_eq!(meta.subject, "subject");
}

#[test]
fn constructors_preserve_model_fields_without_recomputing_at_call_sites() {
    let meta = CommitMeta::new("abcdef123456", 99, "topic");
    let annotation = BranchAnnotation::new(meta.clone(), 2);
    let point = BranchPoint::new("abcdef123456", ["feature"], Some(annotation.clone()));
    let rewritten = RewrittenCommit::new(meta.clone(), CommitMeta::new("fedcba654321", 98, "new"));
    let lane = Lane::new(
        "abcdef123456",
        Some("base".to_string()),
        vec![point],
        99,
        true,
    );
    let group = LaneGroup::new(
        Some("base".to_string()),
        Some(meta),
        Some(1),
        vec![lane.clone()],
    );
    let repository = RepositorySnapshot::new(
        Some("feature".to_string()),
        Some("abcdef123456".to_string()),
        "main",
    );

    assert_eq!(annotation.commit_count, 2);
    assert_eq!(rewritten.replacement.subject, "new");
    assert_eq!(lane.head_oid, "abcdef123456");
    assert_eq!(group.lanes, vec![lane]);
    assert_eq!(repository.main_name, "main");
}

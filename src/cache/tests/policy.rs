use super::*;

#[test]
fn scheduling_obeys_both_thresholds_backoff_and_clock_reversal() {
    let mut state = MaintenanceState::default();
    state.record(false, u64::MAX);
    assert!(!state.due(INTERVAL));
    state.record(true, 1);
    assert!(!state.due(INTERVAL));
    state.record(true, 4 * ENTRY_LIMIT);
    assert!(state.due(INTERVAL));
    state.reserve(INTERVAL);
    assert!(!state.due(INTERVAL - 1));
    assert!(!state.due(2 * INTERVAL - 1));
    assert!(state.due(2 * INTERVAL));
    assert!(!MaintenanceState::completed(2 * INTERVAL, false).due(3 * INTERVAL));
    assert!(MaintenanceState::completed(2 * INTERVAL, true).due(3 * INTERVAL));
    let mut frequent = MaintenanceState::default();
    for _ in 0..16 {
        frequent.record(true, 0);
    }
    assert!(frequent.due(INTERVAL));
    frequent.publications = u64::MAX;
    frequent.bytes_published = u64::MAX;
    frequent.record(true, 1);
    assert_eq!(frequent.publications, u64::MAX);
    assert_eq!(frequent.bytes_published, u64::MAX);
}

#[test]
fn retention_respects_current_snapshot_age_size_recency_and_quota() {
    assert!(!should_evict(9, 0, ENTRY_LIMIT + 1, true, MAX_AGE));
    assert!(should_evict(8, MAX_AGE, 0, false, MAX_AGE));
    assert!(should_evict(0, MAX_AGE, ENTRY_LIMIT + 1, false, MAX_AGE));
    assert!(should_evict(0, 0, ENTRY_LIMIT, false, MAX_AGE));
    assert!(!should_evict(0, 1, ENTRY_LIMIT, false, MAX_AGE));
    assert!(!should_evict(0, MAX_AGE + 1, 0, false, MAX_AGE));
    assert!(within_reclamation_budget(PASS_BYTES - 1, 1));
    assert!(!within_reclamation_budget(PASS_BYTES, 1));
    assert!(!within_reclamation_budget(u64::MAX, 1));
}

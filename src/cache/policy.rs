//! Pure maintenance decisions; callers supply observed time and storage state.
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::ENTRY_LIMIT;

pub(super) const PASS_TIME: Duration = Duration::from_millis(100);
pub(super) const PASS_BYTES: u64 = 8 * ENTRY_LIMIT;
const INTERVAL: u64 = 3600;
const MAX_AGE: u64 = 7 * 24 * 3600;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct MaintenanceState {
    pub(super) publications: u64,
    pub(super) last_attempt: u64,
    #[serde(default)]
    pub(super) bytes_published: u64,
}

impl MaintenanceState {
    pub(super) fn record(&mut self, published: bool, bytes: u64) {
        self.publications = self.publications.saturating_add(u64::from(published));
        if published {
            self.bytes_published = self.bytes_published.saturating_add(bytes);
        }
    }

    pub(super) const fn due(&self, current: u64) -> bool {
        (self.publications >= 16 || self.bytes_published >= 4 * ENTRY_LIMIT)
            && current.saturating_sub(self.last_attempt) >= INTERVAL
    }

    pub(super) const fn reserve(&mut self, current: u64) {
        self.last_attempt = current;
        self.publications = 16;
    }

    pub(super) fn completed(current: u64) -> Self {
        Self {
            last_attempt: current,
            ..Self::default()
        }
    }
}

pub(super) const fn within_reclamation_budget(reclaimed: u64, next_bytes: u64) -> bool {
    reclaimed.saturating_add(next_bytes) <= PASS_BYTES
}

pub(super) fn should_evict(
    index: usize,
    modified: u64,
    bytes: u64,
    is_current: bool,
    current: u64,
) -> bool {
    !is_current
        && (index >= 8
            || bytes > ENTRY_LIMIT
            || current
                .checked_sub(modified)
                .is_some_and(|age| age >= MAX_AGE))
}

#[cfg(test)]
mod tests {
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
        assert!(!MaintenanceState::completed(2 * INTERVAL).due(3 * INTERVAL));
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
}

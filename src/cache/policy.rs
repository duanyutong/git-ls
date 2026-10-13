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

    pub(super) fn completed(current: u64, pending: bool) -> Self {
        Self {
            last_attempt: current,
            publications: if pending { 16 } else { 0 },
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
#[path = "tests/policy.rs"]
mod tests;

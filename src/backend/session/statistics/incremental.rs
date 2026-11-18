//! Conservative correction of a validated aggregate through independent path subtotals.

use super::{Session, StatisticsSelection};
use crate::backend::{DiffBatch, DiffContext, DiffPair};
use crate::model::DiffStat;

fn corrected(total: DiffStat, old: DiffStat, new: DiffStat) -> Option<DiffStat> {
    Some(DiffStat {
        added: total.added.checked_sub(old.added)?.checked_add(new.added)?,
        deleted: total
            .deleted
            .checked_sub(old.deleted)?
            .checked_add(new.deleted)?,
    })
}

impl Session<'_> {
    pub(in crate::backend::session::statistics) fn incremental_statistics(
        &self,
        missing: &[DiffPair],
        selection: &StatisticsSelection<'_>,
        context: &DiffContext,
    ) -> Option<DiffBatch> {
        let [current] = missing else { return None };
        if !selection.pairs.contains(current) {
            return None;
        }
        let transition = self.diff_transitions.borrow().get(&current.key())?.clone();
        let total = {
            let facts = self.facts.borrow();
            let previous = facts
                .diff
                .scopes
                .get(&context.scope)?
                .values
                .get(&transition.previous.key())?;
            let changed = transition.paths.len();
            let bytes = transition
                .paths
                .iter()
                .map(|path| path.len().saturating_add(16))
                .fold(0usize, usize::saturating_add);
            if previous.paths.len() < 64
                || changed == 0
                || changed > 128
                || changed.saturating_mul(4) >= previous.paths.len()
                || bytes > 32 * 1024
                || previous.configuration != context.configuration
                || previous
                    .paths
                    .iter()
                    .any(|path| selection.paths.binary_search(path).is_err())
                || previous.fingerprint != context.fingerprint(&previous.paths)
            {
                return None;
            }
            previous.stat
        };
        let batch =
            match self
                .inner
                .incremental_stats(&transition.previous, current, &transition.paths)
            {
                Ok(Some(batch)) => batch,
                Ok(None) => return None,
                Err(_) => {
                    crate::diagnostics::event(format_args!(
                        "diff_incremental status=bypass reason=optional_comparison_failed"
                    ));
                    return None;
                }
            };
        let [old, new] = batch.stats.as_slice() else {
            crate::diagnostics::event(format_args!(
                "diff_incremental status=bypass reason=invalid_subtotals"
            ));
            return None;
        };
        if batch.paths.as_slice() != transition.paths.as_slice() {
            crate::diagnostics::event(format_args!(
                "diff_incremental status=bypass reason=uncovered_paths"
            ));
            return None;
        }
        let Some(stat) = corrected(total, *old, *new) else {
            crate::diagnostics::event(format_args!(
                "diff_incremental status=bypass reason=invalid_arithmetic"
            ));
            return None;
        };
        crate::diagnostics::count("diff_incremental_reuses", 1);
        Some(DiffBatch {
            stats: vec![stat],
            paths: batch.paths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_rejects_underflow_and_overflow_in_either_dimension() {
        let total = DiffStat {
            added: 20,
            deleted: 30,
        };
        assert_eq!(
            corrected(
                total,
                DiffStat {
                    added: 10,
                    deleted: 5
                },
                DiffStat {
                    added: 2,
                    deleted: 1
                }
            ),
            Some(DiffStat {
                added: 12,
                deleted: 26
            })
        );
        for (old, new) in [
            (
                DiffStat {
                    added: 21,
                    deleted: 0,
                },
                DiffStat::default(),
            ),
            (
                DiffStat {
                    added: 0,
                    deleted: 31,
                },
                DiffStat::default(),
            ),
            (
                DiffStat::default(),
                DiffStat {
                    added: u64::MAX,
                    deleted: 0,
                },
            ),
            (
                DiffStat::default(),
                DiffStat {
                    added: 0,
                    deleted: u64::MAX,
                },
            ),
        ] {
            assert_eq!(corrected(total, old, new), None);
        }
    }
}

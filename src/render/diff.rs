use crate::model::{BranchPoint, LaneGroup};

use super::RenderContext;

#[derive(Clone, Copy, Default)]
pub(super) struct DiffStatWidths {
    added: usize,
    deleted: usize,
}

impl DiffStatWidths {
    pub(super) fn from_groups(groups: &[LaneGroup]) -> Self {
        let mut widths = Self::default();
        for stat in groups
            .iter()
            .flat_map(|group| &group.lanes)
            .flat_map(|lane| &lane.branch_points)
            .filter_map(|point| point.diff_stat)
        {
            widths.added = widths.added.max(count_width(stat.added));
            widths.deleted = widths.deleted.max(count_width(stat.deleted));
        }
        widths
    }
}

const fn count_width(mut count: u64) -> usize {
    let mut width = 2;
    while count >= 10 {
        count /= 10;
        width += 1;
    }
    width
}

pub(super) fn format_diff_stat(
    point: &BranchPoint,
    ctx: &RenderContext<'_>,
    aligned: bool,
) -> String {
    let Some(stat) = point.diff_stat else {
        return String::new();
    };
    let added = format!("+{}", stat.added);
    let deleted = format!("-{}", stat.deleted);
    let added = format!(
        "{added:>width$}",
        width = if aligned {
            ctx.diff_stat_widths.added
        } else {
            0
        }
    );
    let deleted = format!(
        "{deleted:>width$}",
        width = if aligned {
            ctx.diff_stat_widths.deleted
        } else {
            0
        }
    );
    format!(
        "{} {}",
        ctx.colours.diff_added(&added),
        ctx.colours.diff_deleted(&deleted)
    )
}

#[cfg(test)]
#[path = "tests/diff.rs"]
mod tests;

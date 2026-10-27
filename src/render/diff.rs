use crate::model::{BranchPoint, LaneGroup};

use super::{RenderContext, RenderLine};

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

pub(super) fn with_diff_stat(
    line: RenderLine,
    point: &BranchPoint,
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let Some(stat) = point.diff_stat else {
        return line;
    };
    let added = format!("+{}", stat.added);
    let deleted = format!("-{}", stat.deleted);
    let added = format!("{added:>width$}", width = ctx.diff_stat_widths.added);
    let deleted = format!("{deleted:>width$}", width = ctx.diff_stat_widths.deleted);
    let stats = format!(
        "{} {}",
        ctx.colours.diff_added(&added),
        ctx.colours.diff_deleted(&deleted)
    );
    let prefix = line
        .fixed_suffix()
        .map_or(line.as_str(), |(prefix, _)| prefix);
    let subject_width = point
        .annotation
        .as_ref()
        .filter(|_| ctx.verbosity.includes_title())
        .map_or(0, |annotation| {
            console::measure_text_width(&annotation.meta.subject) + 1
        });
    let minimum_prefix_width = console::measure_text_width(prefix).saturating_sub(subject_width)
        + if subject_width > 1 { 4 } else { 0 };
    line.with_diff_stat(&stats, minimum_prefix_width)
}

pub(crate) fn fit_diff_stats(lines: &mut [RenderLine], terminal_width: Option<usize>) {
    if let Some(width) = terminal_width
        && lines
            .iter()
            .filter_map(RenderLine::diff_stat_minimum_width)
            .any(|minimum| minimum > width)
    {
        for line in lines {
            line.hide_diff_stat();
        }
    }
}

#[cfg(test)]
#[path = "tests/diff.rs"]
mod tests;

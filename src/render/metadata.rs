use crate::cli::Verbosity;
use crate::model::{BranchAnnotation, CommitMeta, LaneGroup};

use super::colours::Colours;
use super::context::RenderContext;

pub(super) fn format_age(now_timestamp: i64, commit_timestamp: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;

    let seconds = now_timestamp.saturating_sub(commit_timestamp).max(0);

    match seconds {
        0..MINUTE => format!("{seconds}s"),
        MINUTE..HOUR => format!("{}m", seconds / MINUTE),
        HOUR..DAY => format!("{}h", seconds / HOUR),
        DAY..WEEK => format!("{}d", seconds / DAY),
        WEEK..MONTH => format!("{}w", seconds / WEEK),
        MONTH..YEAR => format!("{}mo", seconds / MONTH),
        _ => format!("{}y", seconds / YEAR),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MetadataWidths {
    pub(super) age: usize,
    pub(super) count: usize,
}

pub(super) fn branch_metadata_columns(
    annotation: &BranchAnnotation,
    now_timestamp: i64,
) -> (String, String) {
    (
        format_age(now_timestamp, annotation.meta.timestamp),
        annotation.commit_count.to_string(),
    )
}

pub(super) fn trunk_metadata_age(meta: &CommitMeta, now_timestamp: i64) -> String {
    format_age(now_timestamp, meta.timestamp)
}

pub(super) fn trunk_count_placeholder(widths: MetadataWidths) -> String {
    "-".repeat(widths.count.max(1))
}

pub(super) fn format_metadata_prefix(
    age: &str,
    count: &str,
    short_oid: &str,
    widths: MetadataWidths,
    colours: &Colours,
) -> String {
    let count_width = widths.count.max(1);
    let age = colours.metadata_age(&format!("{age:>age_width$}", age_width = widths.age));
    let count = colours.metadata_count(&format!("{count:>count_width$}"));
    let short_oid = colours.metadata_oid(short_oid);
    let open = colours.metadata_punctuation("(");
    let comma = colours.metadata_punctuation(", ");
    let close = colours.metadata_punctuation(")");
    format!("{age} {open}{count}{comma}{short_oid}{close}")
}

/// Render the fixed-width age gutter that the columns layout places ahead of the
/// current-row indicator. Returns an empty string for the inline layout, so the
/// same call site serves both layouts. `age` is `None` for rows that carry no
/// commit (spacers, collapsed segments), which still reserve the column width so
/// the topology spine stays vertically aligned.
pub(super) fn age_gutter(ctx: &RenderContext<'_>, age: Option<String>) -> String {
    if !ctx.layout.is_columns() || ctx.metadata_widths.age == 0 {
        return String::new();
    }
    let width = ctx.metadata_widths.age;
    let text = age.unwrap_or_default();
    format!("{} ", ctx.colours.metadata_age(&format!("{text:>width$}")))
}

/// Render the standalone commit-count column used by the columns layout. Genuine
/// branch counts are highlighted; the trunk placeholder is muted so that only
/// real counts draw the eye.
pub(super) fn columns_count(
    count: &str,
    widths: MetadataWidths,
    colours: &Colours,
    is_placeholder: bool,
) -> String {
    let width = widths.count.max(1);
    let text = format!("{count:>width$}");
    if is_placeholder {
        colours.metadata_punctuation(&text)
    } else {
        colours.metadata_count(&text)
    }
}

fn record_metadata_widths(widths: &mut MetadataWidths, age: &str, count: &str) {
    widths.age = widths.age.max(age.len());
    widths.count = widths.count.max(count.len());
}

pub(crate) fn calculate_metadata_widths(
    groups: &[LaneGroup],
    main_meta: Option<&CommitMeta>,
    now_timestamp: i64,
    verbosity: Verbosity,
) -> MetadataWidths {
    if !verbosity.includes_metadata() {
        return MetadataWidths::default();
    }

    let mut widths = MetadataWidths::default();
    if let Some(meta) = main_meta {
        let age = trunk_metadata_age(meta, now_timestamp);
        record_metadata_widths(&mut widths, &age, "");
    }
    for group in groups {
        if let Some(meta) = group.base_meta.as_ref() {
            let age = trunk_metadata_age(meta, now_timestamp);
            record_metadata_widths(&mut widths, &age, "");
        }
        for lane in &group.lanes {
            for point in &lane.branch_points {
                if let Some(annotation) = point.annotation.as_ref() {
                    let (age, count) = branch_metadata_columns(annotation, now_timestamp);
                    record_metadata_widths(&mut widths, &age, &count);
                }
            }
            for commit in &lane.rewritten_commits {
                let age = format_age(now_timestamp, commit.meta.timestamp);
                record_metadata_widths(&mut widths, &age, "");
            }
        }
    }
    widths
}

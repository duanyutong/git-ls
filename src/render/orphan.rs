use crate::cli::{Layout, Verbosity};
use crate::model::{BranchPoint, Lane};

use super::colours::Colours;
use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, COLLAPSED_MAIN_GLYPH, ORPHANED_BRANCH_GLYPH, TREE_LEFT_PADDING,
    is_current_branch_point, orphaned_row_indicator, render_row,
};
use super::line::RenderLine;
use super::metadata::{
    MetadataWidths, age_gutter, branch_metadata_columns, columns_count, format_age,
    format_metadata_prefix,
};

pub(super) fn display_orphaned_names(
    point: &BranchPoint,
    now_timestamp: i64,
    verbosity: Verbosity,
    metadata_widths: MetadataWidths,
    layout: Layout,
    colours: &Colours,
) -> String {
    let names = point
        .names
        .iter()
        .map(|name| colours.orphaned_name(name))
        .collect::<Vec<_>>()
        .join(", ");
    let status = colours.orphaned_status("(orphaned)");

    let Some(annotation) = point
        .annotation
        .as_ref()
        .filter(|_| verbosity.includes_metadata())
    else {
        return format!("{names} {status}");
    };

    let (age, count) = branch_metadata_columns(annotation, now_timestamp);

    if layout.is_columns() {
        let count_column = columns_count(&count, metadata_widths, colours, false);
        let mut label = format!("{count_column} {names} {status}");
        if verbosity.includes_title() {
            label.push(' ');
            label.push_str(&colours.commit_title(&annotation.meta.subject));
        }
        if verbosity.includes_oid() {
            label.push(' ');
            label.push_str(&colours.metadata_oid(&annotation.meta.short_oid));
        }
        return label;
    }

    let prefix = format_metadata_prefix(
        &age,
        &count,
        verbosity
            .includes_oid()
            .then_some(annotation.meta.short_oid.as_str()),
        metadata_widths,
        colours,
    );
    if verbosity.includes_title() {
        format!(
            "{prefix} {names} {status} {}",
            colours.commit_title(&annotation.meta.subject)
        )
    } else {
        format!("{prefix} {names} {status}")
    }
}

pub(super) fn render_orphaned_group(lanes: &[Lane], ctx: &RenderContext<'_>) -> Vec<RenderLine> {
    let mut output = Vec::new();

    for lane in lanes {
        for point in &lane.branch_points {
            let label = display_orphaned_names(
                point,
                ctx.now_timestamp,
                ctx.verbosity,
                ctx.metadata_widths,
                ctx.layout,
                ctx.colours,
            );
            let gutter = age_gutter(
                ctx,
                point
                    .annotation
                    .as_ref()
                    .filter(|_| ctx.verbosity.includes_metadata())
                    .map(|annotation| format_age(ctx.now_timestamp, annotation.meta.timestamp)),
            );
            let line = format!(
                "{TREE_LEFT_PADDING}{} {}{BRANCH_LABEL_GAP}{label}",
                ctx.colours.stack(0, COLLAPSED_MAIN_GLYPH),
                ctx.colours.orphaned_glyph(ORPHANED_BRANCH_GLYPH)
            );
            let rendered = render_row(
                &gutter,
                &orphaned_row_indicator(
                    is_current_branch_point(point, ctx.current_branch),
                    ctx.colours,
                ),
                &line,
            );
            let oid = point
                .annotation
                .as_ref()
                .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_oid())
                .map(|annotation| ctx.colours.metadata_oid(&annotation.meta.short_oid));
            output.push(match oid {
                Some(oid) => RenderLine::with_trailing_fixed_suffix(rendered, oid),
                None => RenderLine::plain(rendered),
            });
        }
    }

    output
}

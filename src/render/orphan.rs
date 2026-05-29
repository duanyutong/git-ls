use crate::cli::Verbosity;
use crate::model::{BranchPoint, Lane};

use super::colours::Colours;
use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, COLLAPSED_MAIN_GLYPH, ORPHANED_BRANCH_GLYPH, TREE_LEFT_PADDING,
    is_current_branch_point, orphaned_row_indicator, render_row,
};
use super::metadata::{MetadataWidths, branch_metadata_columns, format_metadata_prefix};

pub(super) fn display_orphaned_names(
    point: &BranchPoint,
    now_timestamp: i64,
    verbosity: Verbosity,
    metadata_widths: MetadataWidths,
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
    let prefix = format_metadata_prefix(
        &age,
        &count,
        &annotation.meta.short_oid,
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

pub(super) fn render_orphaned_group(lanes: &[Lane], ctx: &RenderContext<'_>) -> Vec<String> {
    let mut output = Vec::new();

    for lane in lanes {
        for point in &lane.branch_points {
            let label = display_orphaned_names(
                point,
                ctx.now_timestamp,
                ctx.verbosity,
                ctx.metadata_widths,
                ctx.colours,
            );
            let line = format!(
                "{TREE_LEFT_PADDING}{} {}{BRANCH_LABEL_GAP}{label}",
                ctx.colours.dim(COLLAPSED_MAIN_GLYPH),
                ctx.colours.orphaned_glyph(ORPHANED_BRANCH_GLYPH)
            );
            output.push(render_row(
                &orphaned_row_indicator(
                    is_current_branch_point(point, ctx.current_branch),
                    ctx.colours,
                ),
                &line,
            ));
        }
    }

    output
}

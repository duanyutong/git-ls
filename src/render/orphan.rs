use crate::model::Lane;

use super::branch::BranchLabel;
use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, COLLAPSED_MAIN_GLYPH, ORPHANED_BRANCH_GLYPH, TREE_LEFT_PADDING,
    is_current_branch_point, orphaned_row_indicator, render_row,
};
use super::line::RenderLine;
use super::metadata::{age_gutter, format_age};

pub(super) fn render_orphaned_group(lanes: &[Lane], ctx: &RenderContext<'_>) -> Vec<RenderLine> {
    let mut output = Vec::new();

    for lane in lanes {
        for point in &lane.branch_points {
            let gutter = age_gutter(
                ctx,
                point
                    .annotation
                    .as_ref()
                    .filter(|_| ctx.verbosity.includes_metadata())
                    .map(|annotation| format_age(ctx.now_timestamp, annotation.meta.timestamp)),
            );
            let line = format!(
                "{TREE_LEFT_PADDING}{} {}{BRANCH_LABEL_GAP}",
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
            let label = BranchLabel::new(point, 0, true, ctx);
            output.extend(label.render(
                &rendered,
                &|| {
                    render_row(
                        &age_gutter(ctx, None),
                        " ",
                        &format!(
                            "{TREE_LEFT_PADDING}{} {}",
                            ctx.colours.stack(0, COLLAPSED_MAIN_GLYPH),
                            ctx.colours.orphaned_glyph("│")
                        ),
                    )
                },
                ctx.terminal_width,
            ));
        }
    }

    output
}

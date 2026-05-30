use crate::model::{BranchPoint, Lane, LaneGroup, RewrittenCommit};

use super::branch::display_names;
use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, LaneRenderLayout, MainSpine, REWRITTEN_COMMIT_GLYPH, current_row_indicator,
    is_current_branch_point, render_row, row_prefix, row_prefix_with_marker,
};
use super::line::RenderLine;
use super::metadata::{age_gutter, format_age};
use super::orphan::render_orphaned_group;
use super::rewrite::display_rewritten_commit;
use super::trunk::{
    TrunkLabel, main_is_current, render_collapsed_main_segment, render_main_tip,
    render_omitted_main_past, render_top_spacer, trunk_age, trunk_label, trunk_prefix,
};

pub(super) fn render_group(
    lanes: &[Lane],
    lane_field_width: usize,
    colour_offset: usize,
    ctx: &RenderContext<'_>,
    label: TrunkLabel<'_>,
    main_spine: MainSpine,
) -> Vec<RenderLine> {
    let lane_count = lanes.len();
    let layout = LaneRenderLayout::new(lane_count, lane_field_width, colour_offset);
    let content_row_count: usize = lanes
        .iter()
        .map(|lane| lane.branch_points.len() + lane.rewritten_commits.len())
        .sum();
    let mut rendered_content_rows = 0;
    let mut output = Vec::new();

    for (lane_index, lane) in lanes.iter().enumerate() {
        let colour_index = colour_offset + lane_index;
        for point in &lane.branch_points {
            rendered_content_rows += 1;
            let spine = row_main_spine(main_spine, rendered_content_rows, content_row_count);
            output.push(render_branch_point_row(
                point,
                lane_index,
                colour_index,
                layout,
                spine,
                ctx,
            ));
        }

        for commit in &lane.rewritten_commits {
            rendered_content_rows += 1;
            let spine = row_main_spine(main_spine, rendered_content_rows, content_row_count);
            output.push(render_rewritten_row(
                commit,
                lane_index,
                colour_index,
                layout,
                spine,
                ctx,
            ));
        }
    }

    output.push(render_trunk_row(label, layout, main_spine, ctx));
    output
}

/// The trailing branch-only row of a `Future` group collapses its spine to a
/// dotted continuation; every other row keeps the group's spine state.
fn row_main_spine(main_spine: MainSpine, rendered: usize, total: usize) -> MainSpine {
    if matches!(main_spine, MainSpine::Future) && rendered == total {
        MainSpine::FutureLine
    } else {
        main_spine
    }
}

fn branch_age(annotation_age: Option<i64>, ctx: &RenderContext<'_>) -> Option<String> {
    annotation_age
        .filter(|_| ctx.verbosity.includes_metadata())
        .map(|timestamp| format_age(ctx.now_timestamp, timestamp))
}

fn render_branch_point_row(
    point: &BranchPoint,
    lane_index: usize,
    colour_index: usize,
    layout: LaneRenderLayout,
    main_spine: MainSpine,
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let prefix = row_prefix(
        lane_index,
        layout,
        point,
        ctx.current_branch,
        ctx.head,
        main_spine,
        ctx.colours,
    );
    let label = display_names(
        point,
        ctx.current_branch,
        colour_index,
        ctx.now_timestamp,
        ctx.verbosity,
        ctx.metadata_widths,
        ctx.layout,
        ctx.colours,
    );
    let gutter = age_gutter(
        ctx,
        branch_age(point.annotation.as_ref().map(|a| a.meta.timestamp), ctx),
    );
    let line = format!("{prefix}{BRANCH_LABEL_GAP}{label}");
    let rendered = render_row(
        &gutter,
        &current_row_indicator(
            is_current_branch_point(point, ctx.current_branch),
            colour_index,
            ctx.colours,
        ),
        &line,
    );
    let oid = point
        .annotation
        .as_ref()
        .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_metadata())
        .map(|annotation| ctx.colours.metadata_oid(&annotation.meta.short_oid));
    match oid {
        Some(oid) => RenderLine::with_trailing_fixed_suffix(rendered, oid),
        None => RenderLine::plain(rendered),
    }
}

fn render_rewritten_row(
    commit: &RewrittenCommit,
    lane_index: usize,
    colour_index: usize,
    layout: LaneRenderLayout,
    main_spine: MainSpine,
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let prefix = row_prefix_with_marker(
        lane_index,
        layout,
        REWRITTEN_COMMIT_GLYPH,
        main_spine,
        ctx.colours,
    );
    let label = display_rewritten_commit(commit, ctx);
    let gutter = age_gutter(ctx, branch_age(Some(commit.meta.timestamp), ctx));
    let line = format!("{prefix}{BRANCH_LABEL_GAP}{label}");
    RenderLine::plain(render_row(
        &gutter,
        &current_row_indicator(false, colour_index, ctx.colours),
        &line,
    ))
}

fn render_trunk_row(
    label: TrunkLabel<'_>,
    layout: LaneRenderLayout,
    main_spine: MainSpine,
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let current_main =
        matches!(label, TrunkLabel::Main) && main_is_current(ctx.main_name, ctx.current_branch);
    let gutter = age_gutter(ctx, trunk_age(label, ctx));
    let rendered = trunk_label(label, ctx);
    let line = format!(
        "{}{BRANCH_LABEL_GAP}{}",
        trunk_prefix(layout, current_main, main_spine, ctx.colours),
        rendered
    );
    let rendered = render_row(
        &gutter,
        &current_row_indicator(current_main, 0, ctx.colours),
        &line,
    );
    let oid = match label {
        TrunkLabel::Main => ctx.main_meta,
        TrunkLabel::Commit(meta) => Some(meta),
    }
    .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_metadata())
    .map(|meta| ctx.colours.metadata_oid(&meta.short_oid));
    match oid {
        Some(oid) => RenderLine::with_trailing_fixed_suffix(rendered, oid),
        None => RenderLine::plain(rendered),
    }
}

fn connected_lane_field_width(groups: &[LaneGroup]) -> usize {
    groups
        .iter()
        .filter(|group| group.main_distance.is_some())
        .map(|group| group.lanes.len())
        .max()
        .unwrap_or(0)
}

pub(crate) fn render_lane_groups(groups: &[LaneGroup], ctx: &RenderContext<'_>) -> Vec<RenderLine> {
    let mut output = Vec::new();
    let mut colour_offset = 1;
    let mut connected_started = false;
    let mut rendered_connected_group = false;
    let mut previous_main_distance = 0;
    let lane_field_width = connected_lane_field_width(groups);

    for group in groups {
        if let Some(main_distance) = group.main_distance {
            if !connected_started {
                output.push(render_top_spacer(
                    ctx,
                    main_distance == 0 && !group.lanes.is_empty(),
                ));
                if main_distance > 0 {
                    output.push(render_main_tip(ctx));
                }
                connected_started = true;
            }
            if connected_started && main_distance > previous_main_distance {
                output.extend(render_collapsed_main_segment(
                    main_distance - previous_main_distance,
                    ctx,
                ));
            }

            let label = match (main_distance, group.base_meta.as_ref()) {
                (0, _) | (_, None) => TrunkLabel::Main,
                (_, Some(base_meta)) => TrunkLabel::Commit(base_meta),
            };
            let main_spine = if main_distance == 0 {
                MainSpine::Future
            } else {
                MainSpine::Connected
            };
            output.extend(render_group(
                &group.lanes,
                lane_field_width,
                colour_offset,
                ctx,
                label,
                main_spine,
            ));
            rendered_connected_group = true;
            previous_main_distance = main_distance;
            connected_started = true;
        } else {
            if !rendered_connected_group && !connected_started {
                output.push(render_top_spacer(ctx, false));
                output.push(render_main_tip(ctx));
                connected_started = true;
            }
            output.extend(render_orphaned_group(&group.lanes, ctx));
            continue;
        }

        colour_offset += group.lanes.len();
    }

    if !output.is_empty() {
        output.push(render_omitted_main_past(ctx));
    }

    output
}

pub(crate) fn render_empty_selection(ctx: &RenderContext<'_>) -> Vec<RenderLine> {
    vec![
        render_top_spacer(ctx, false),
        render_main_tip(ctx),
        render_omitted_main_past(ctx),
    ]
}

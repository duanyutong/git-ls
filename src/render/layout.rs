use crate::model::{Lane, LaneGroup};

use super::branch::display_names;
use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, LaneRenderLayout, MainSpine, REWRITTEN_COMMIT_GLYPH, current_row_indicator,
    is_current_branch_point, render_row, row_prefix, row_prefix_with_marker,
};
use super::orphan::render_orphaned_group;
use super::rewrite::display_rewritten_commit;
use super::trunk::{
    TrunkLabel, main_is_current, render_collapsed_main_segment, render_main_tip,
    render_omitted_main_past, render_top_spacer, trunk_label, trunk_prefix,
};

pub(super) fn render_group(
    lanes: &[Lane],
    lane_field_width: usize,
    colour_offset: usize,
    ctx: &RenderContext<'_>,
    label: TrunkLabel<'_>,
    main_spine: MainSpine,
) -> Vec<String> {
    let lane_count = lanes.len();
    let layout = LaneRenderLayout::new(lane_count, lane_field_width, colour_offset);
    let content_row_count: usize = lanes
        .iter()
        .map(|lane| lane.branch_points.len() + lane.rewritten_commits.len())
        .sum();
    let mut rendered_content_rows = 0;
    let mut output = Vec::new();

    for (lane_index, lane) in lanes.iter().enumerate() {
        for point in &lane.branch_points {
            rendered_content_rows += 1;
            let row_main_spine = if matches!(main_spine, MainSpine::Future)
                && rendered_content_rows == content_row_count
            {
                MainSpine::FutureLine
            } else {
                main_spine
            };
            let colour_index = colour_offset + lane_index;
            let prefix = row_prefix(
                lane_index,
                layout,
                point,
                ctx.current_branch,
                ctx.head,
                row_main_spine,
                ctx.colours,
            );
            let label = display_names(
                point,
                ctx.current_branch,
                colour_index,
                ctx.now_timestamp,
                ctx.verbosity,
                ctx.metadata_widths,
                ctx.colours,
            );
            let line = format!("{prefix}{BRANCH_LABEL_GAP}{label}");
            output.push(render_row(
                &current_row_indicator(
                    is_current_branch_point(point, ctx.current_branch),
                    colour_index,
                    ctx.colours,
                ),
                &line,
            ));
        }

        for commit in &lane.rewritten_commits {
            rendered_content_rows += 1;
            let row_main_spine = if matches!(main_spine, MainSpine::Future)
                && rendered_content_rows == content_row_count
            {
                MainSpine::FutureLine
            } else {
                main_spine
            };
            let colour_index = colour_offset + lane_index;
            let prefix = row_prefix_with_marker(
                lane_index,
                layout,
                REWRITTEN_COMMIT_GLYPH,
                row_main_spine,
                ctx.colours,
            );
            let label = display_rewritten_commit(commit, ctx);
            let line = format!("{prefix}{BRANCH_LABEL_GAP}{label}");
            output.push(render_row(
                &current_row_indicator(false, colour_index, ctx.colours),
                &line,
            ));
        }
    }

    let current_main =
        matches!(label, TrunkLabel::Main) && main_is_current(ctx.main_name, ctx.current_branch);
    let label = trunk_label(label, ctx);
    let line = format!(
        "{}{BRANCH_LABEL_GAP}{}",
        trunk_prefix(layout, current_main, main_spine, ctx.colours),
        label
    );
    output.push(render_row(
        &current_row_indicator(current_main, 0, ctx.colours),
        &line,
    ));
    output
}

fn connected_lane_field_width(groups: &[LaneGroup]) -> usize {
    groups
        .iter()
        .filter(|group| group.main_distance.is_some())
        .map(|group| group.lanes.len())
        .max()
        .unwrap_or(0)
}

pub(crate) fn render_lane_groups(groups: &[LaneGroup], ctx: &RenderContext<'_>) -> Vec<String> {
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
                    ctx.colours,
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
                output.push(render_top_spacer(ctx.colours, false));
                output.push(render_main_tip(ctx));
                connected_started = true;
            }
            output.extend(render_orphaned_group(&group.lanes, ctx));
            continue;
        }

        colour_offset += group.lanes.len();
    }

    if !output.is_empty() {
        output.push(render_omitted_main_past(ctx.colours));
    }

    output
}

pub(crate) fn render_empty_selection(ctx: &RenderContext<'_>) -> Vec<String> {
    vec![
        render_top_spacer(ctx.colours, false),
        render_main_tip(ctx),
        render_omitted_main_past(ctx.colours),
    ]
}

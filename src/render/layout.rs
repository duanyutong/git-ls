use crate::model::{BranchPoint, Lane, LaneGroup, RewrittenCommit};

use super::ancestry::{Content, shared_ancestry};
use super::branch::display_names;
use super::context::RenderContext;
use super::diff::with_diff_stat;
use super::graph::{
    BRANCH_LABEL_GAP, LaneRenderLayout, MainSpine, REWRITTEN_COMMIT_GLYPH, current_row_indicator,
    is_current_branch_point, junction_prefix, marker_for, render_row, row_prefix_with_marker,
};
use super::line::RenderLine;
use super::metadata::{age_gutter, format_age};
use super::orphan::render_orphaned_group;
use super::rewrite::display_rewritten_commit;
use super::trunk::{
    TrunkLabel, reference_is_current, reference_label, render_collapsed_main_segment,
    render_main_tip, render_omitted_main_past, render_top_spacer, trunk_age, trunk_is_current,
    trunk_label, trunk_prefix_with_roots,
};

pub(super) fn render_group(
    lanes: &[Lane],
    lane_field_width: usize,
    next_colour: &mut usize,
    ctx: &RenderContext<'_>,
    label: TrunkLabel<'_>,
    main_spine: MainSpine,
) -> Vec<RenderLine> {
    let lane_count = lanes.len();
    let layout = LaneRenderLayout::new(lane_count, lane_field_width, *next_colour)
        .with_width(ctx.terminal_width);
    let ancestry = ctx
        .ancestry
        .map_or_else(|| shared_ancestry(lanes), |prepared| prepared.bind(lanes));
    let mut active = vec![false; lane_count];
    let mut lane_colours = vec![0; lane_count];
    let mut output = Vec::new();

    for (index, row) in ancestry.rows.iter().enumerate() {
        if row.children.len() > 1 {
            let prefix = junction_prefix(
                layout,
                main_spine,
                &row.children,
                &active,
                &lane_colours,
                ctx.colours,
            );
            output.push(RenderLine::plain(render_row(
                &age_gutter(ctx, None),
                " ",
                &prefix,
            )));
            for &child in &row.children {
                active[child] = false;
            }
        }
        if !active[row.lane] {
            lane_colours[row.lane] = *next_colour;
            *next_colour += 1;
        }
        active[row.lane] = true;
        let spine = row_main_spine(main_spine, index + 1, ancestry.rows.len());
        let marker = match row.content {
            Content::Branch(point) => {
                if point.main_history {
                    if reference_is_current(&point.oid, ctx) {
                        super::graph::CURRENT_MAIN_COMMIT_GLYPH
                    } else {
                        super::graph::MAIN_COMMIT_GLYPH
                    }
                } else {
                    marker_for(point, ctx.current_branch, ctx.head)
                }
            }
            Content::Rewritten(_) => REWRITTEN_COMMIT_GLYPH,
        };
        let prefix = row_prefix_with_marker(
            row.lane,
            layout,
            marker,
            spine,
            &active,
            &lane_colours,
            ctx.colours,
        );
        let colour_index = lane_colours[row.lane];
        output.push(match row.content {
            Content::Branch(point) => render_branch_point_row(point, colour_index, &prefix, ctx),
            Content::Rewritten(commit) => render_rewritten_row(commit, colour_index, &prefix, ctx),
        });
    }

    output.push(render_trunk_row(
        label,
        layout,
        main_spine,
        &ancestry.roots,
        &lane_colours,
        ctx,
    ));
    output
}

/// The trailing branch-only row of a `Future` group collapses its spine to a
/// dotted continuation; every other row keeps the group's spine state.
const fn row_main_spine(main_spine: MainSpine, rendered: usize, total: usize) -> MainSpine {
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
    colour_index: usize,
    prefix: &str,
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let reference = reference_label(
        &point.oid,
        point.annotation.as_ref().map(|annotation| &annotation.meta),
        ctx,
    );
    let is_main_reference = reference.is_some() || point.main_history;
    let label = reference.unwrap_or_else(|| {
        if point.main_history
            && let Some(annotation) = &point.annotation
        {
            return trunk_label(TrunkLabel::Commit(&annotation.meta), ctx);
        }
        display_names(
            point,
            ctx.current_branch,
            colour_index,
            ctx.now_timestamp,
            ctx.verbosity,
            ctx.metadata_widths,
            ctx.layout,
            ctx.colours,
        )
    });
    let gutter = age_gutter(
        ctx,
        branch_age(point.annotation.as_ref().map(|a| a.meta.timestamp), ctx),
    );
    let line = format!("{prefix}{BRANCH_LABEL_GAP}{label}");
    let rendered = render_row(
        &gutter,
        &current_row_indicator(
            if is_main_reference {
                reference_is_current(&point.oid, ctx)
            } else {
                is_current_branch_point(point, ctx.current_branch)
            },
            colour_index,
            ctx.colours,
        ),
        &line,
    );
    let oid = point
        .annotation
        .as_ref()
        .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_oid())
        .map(|annotation| ctx.colours.metadata_oid(&annotation.meta.short_oid));
    let line = match oid {
        Some(oid) => RenderLine::with_trailing_fixed_suffix(rendered, oid),
        None => RenderLine::plain(rendered),
    };
    if is_main_reference {
        line
    } else {
        with_diff_stat(line, point, ctx)
    }
}

fn render_rewritten_row(
    commit: &RewrittenCommit,
    colour_index: usize,
    prefix: &str,
    ctx: &RenderContext<'_>,
) -> RenderLine {
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
    roots: &[usize],
    lane_colours: &[usize],
    ctx: &RenderContext<'_>,
) -> RenderLine {
    let current_main = trunk_is_current(label, ctx);
    let gutter = age_gutter(ctx, trunk_age(label, ctx));
    let rendered = trunk_label(label, ctx);
    let line = format!(
        "{}{BRANCH_LABEL_GAP}{}",
        trunk_prefix_with_roots(
            layout,
            current_main,
            main_spine,
            roots,
            lane_colours,
            ctx.colours
        ),
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
    .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_oid())
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
    let colours = ctx.colours.for_groups(groups);
    let ctx = &RenderContext {
        colours: &colours,
        ..*ctx
    };
    let mut output = Vec::new();
    let mut next_colour = 1;
    let mut connected_started = false;
    let mut rendered_connected_group = false;
    let mut previous_main_distance = 0;
    let mut history_name = ctx
        .main_refs
        .iter()
        .find(|reference| Some(reference.oid.as_str()) == ctx.main_oid)
        .map_or(ctx.main_name, |reference| reference.name.as_str());
    let lane_field_width = connected_lane_field_width(groups);

    for (index, group) in groups.iter().enumerate() {
        let group_context = RenderContext {
            ancestry: ctx.topology.and_then(|topology| topology.groups.get(index)),
            ..*ctx
        };
        let ctx = &group_context;
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
                let gap_context = RenderContext {
                    main_history_name: Some(history_name),
                    ..*ctx
                };
                output.extend(render_collapsed_main_segment(
                    main_distance - previous_main_distance,
                    &gap_context,
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
                &mut next_colour,
                ctx,
                label,
                main_spine,
            ));
            rendered_connected_group = true;
            previous_main_distance = main_distance;
            if let Some(reference) = ctx
                .main_refs
                .iter()
                .find(|reference| Some(&reference.oid) == group.base_oid.as_ref())
            {
                history_name = &reference.name;
            }
            connected_started = true;
        } else {
            if !rendered_connected_group && !connected_started {
                output.push(render_top_spacer(ctx, false));
                output.push(render_main_tip(ctx));
                connected_started = true;
            }
            output.extend(render_orphaned_group(&group.lanes, ctx));
        }
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

use crate::model::CommitMeta;

use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, COLLAPSED_MAIN_GLYPH, CURRENT_MAIN_COMMIT_GLYPH, LaneRenderLayout,
    MAIN_COMMIT_GLYPH, MAIN_SPINE_GLYPH, MainSpine, TREE_LEFT_PADDING, current_row_indicator,
    render_row,
};
use super::metadata::{format_metadata_prefix, trunk_count_placeholder, trunk_metadata_age};

pub(super) fn main_is_current(main_name: &str, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| branch == main_name)
}

pub(super) fn main_label(ctx: &RenderContext<'_>) -> String {
    let name = if main_is_current(ctx.main_name, ctx.current_branch) {
        ctx.colours.current_stack(0, ctx.main_name)
    } else {
        ctx.colours.stack(0, ctx.main_name)
    };

    let Some(meta) = ctx.main_meta.filter(|_| ctx.verbosity.includes_metadata()) else {
        return name;
    };

    let age = trunk_metadata_age(meta, ctx.now_timestamp);
    let count = trunk_count_placeholder(ctx.metadata_widths);
    let prefix = format_metadata_prefix(
        &age,
        &count,
        &meta.short_oid,
        ctx.metadata_widths,
        ctx.colours,
    );
    format!("{prefix} {name}")
}

pub(super) fn trunk_prefix(
    layout: LaneRenderLayout,
    main_is_current: bool,
    main_spine: MainSpine,
    colours: &super::colours::Colours,
) -> String {
    let marker = if main_is_current {
        colours.stack(0, CURRENT_MAIN_COMMIT_GLYPH)
    } else {
        colours.stack(0, MAIN_COMMIT_GLYPH)
    };

    if layout.lane_count == 0 {
        let extension = colours.stack(0, "──");
        return format!("{TREE_LEFT_PADDING}{marker}{extension}");
    }

    if !main_spine.is_connected() {
        return format!("{TREE_LEFT_PADDING}{marker}");
    }

    let mut parts = vec![marker];
    for _ in 0..layout.lane_padding() {
        let colour_index = layout.colour_offset;
        parts.push(colours.stack(colour_index, "──"));
    }
    for index in 0..layout.lane_count {
        let glyph = if index + 1 == layout.lane_count {
            "─┘"
        } else {
            "─┴"
        };
        parts.push(colours.stack(layout.colour_offset + index, glyph));
    }
    format!("{TREE_LEFT_PADDING}{}", parts.join(""))
}

#[derive(Clone, Copy)]
pub(super) enum TrunkLabel<'a> {
    Main,
    Commit(&'a CommitMeta),
}

pub(super) fn trunk_label(label: TrunkLabel<'_>, ctx: &RenderContext<'_>) -> String {
    match label {
        TrunkLabel::Main => main_label(ctx),
        TrunkLabel::Commit(meta) => {
            let subject = ctx.colours.commit_title(&meta.subject);
            if !ctx.verbosity.includes_metadata() {
                return subject;
            }

            let age = trunk_metadata_age(meta, ctx.now_timestamp);
            let count = trunk_count_placeholder(ctx.metadata_widths);
            let prefix = format_metadata_prefix(
                &age,
                &count,
                &meta.short_oid,
                ctx.metadata_widths,
                ctx.colours,
            );
            format!("{prefix} {subject}")
        }
    }
}

pub(crate) fn render_main_tip(ctx: &RenderContext<'_>) -> String {
    let current_main = main_is_current(ctx.main_name, ctx.current_branch);
    let line = format!(
        "{}{BRANCH_LABEL_GAP}{}",
        trunk_prefix(
            LaneRenderLayout::empty(),
            current_main,
            MainSpine::Hidden,
            ctx.colours
        ),
        main_label(ctx)
    );
    render_row(&current_row_indicator(current_main, 0, ctx.colours), &line)
}

pub(crate) fn render_top_spacer(
    colours: &super::colours::Colours,
    has_visible_rows_above_main: bool,
) -> String {
    if has_visible_rows_above_main {
        String::new()
    } else {
        render_omitted_main_past(colours)
    }
}

pub(super) fn render_collapsed_main_segment(
    commit_count: usize,
    ctx: &RenderContext<'_>,
) -> impl IntoIterator<Item = String> {
    let noun = if commit_count == 1 {
        "commit"
    } else {
        "commits"
    };
    let label = format!("({commit_count} {noun} on {})", ctx.main_name);
    [
        render_row(
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{}",
                ctx.colours.stack(0, MAIN_SPINE_GLYPH)
            ),
        ),
        render_row(
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{} {}",
                ctx.colours.stack(0, COLLAPSED_MAIN_GLYPH),
                ctx.colours.dim(&label)
            ),
        ),
        render_row(
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{}",
                ctx.colours.stack(0, MAIN_SPINE_GLYPH)
            ),
        ),
    ]
}

pub(crate) fn render_omitted_main_past(colours: &super::colours::Colours) -> String {
    let line = format!(
        "{TREE_LEFT_PADDING}{}",
        colours.stack(0, COLLAPSED_MAIN_GLYPH)
    );
    render_row(" ", &line)
}

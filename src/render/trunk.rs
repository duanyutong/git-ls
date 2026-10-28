use crate::model::CommitMeta;

use super::context::RenderContext;
use super::graph::{
    BRANCH_LABEL_GAP, COLLAPSED_MAIN_GLYPH, CURRENT_MAIN_COMMIT_GLYPH, LaneRenderLayout,
    MAIN_COMMIT_GLYPH, MAIN_SPINE_GLYPH, MainSpine, TREE_LEFT_PADDING, current_row_indicator,
    render_row,
};
use super::line::RenderLine;
use super::metadata::{
    age_gutter, columns_count, format_metadata_prefix, trunk_count_placeholder, trunk_metadata_age,
};

pub(super) fn main_is_current(main_name: &str, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| branch == main_name)
}

pub(super) fn main_label(ctx: &RenderContext<'_>) -> String {
    if let Some(label) = reference_label(ctx.main_oid.unwrap_or_default(), ctx.main_meta, ctx) {
        return label;
    }
    let name = if main_is_current(ctx.main_name, ctx.current_branch) {
        ctx.colours.current_stack(0, ctx.main_name)
    } else {
        ctx.colours.stack(0, ctx.main_name)
    };
    named_trunk_label(name, ctx.main_meta, ctx)
}

pub(super) fn reference_label(
    oid: &str,
    meta: Option<&CommitMeta>,
    ctx: &RenderContext<'_>,
) -> Option<String> {
    let names: Vec<_> = ctx
        .main_refs
        .iter()
        .filter(|reference| reference.oid == oid)
        .map(|reference| {
            let mut name =
                if ctx.current_branch == Some(reference.name.as_str()) && !reference.cached {
                    ctx.colours.current_stack(0, &reference.name)
                } else {
                    ctx.colours.stack(0, &reference.name)
                };
            if reference.cached {
                name.push_str(&ctx.colours.dim(" (cached)"));
            }
            name
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    Some(named_trunk_label(names.join(", "), meta, ctx))
}

pub(super) fn trunk_is_current(label: TrunkLabel<'_>, ctx: &RenderContext<'_>) -> bool {
    if ctx.main_refs.is_empty() {
        return matches!(label, TrunkLabel::Main)
            && main_is_current(ctx.main_name, ctx.current_branch);
    }
    let oid = match label {
        TrunkLabel::Main => ctx.main_oid,
        TrunkLabel::Commit(meta) => Some(meta.oid.as_str()),
    };
    oid.is_some_and(|oid| reference_is_current(oid, ctx))
}

pub(super) fn reference_is_current(oid: &str, ctx: &RenderContext<'_>) -> bool {
    ctx.main_refs.iter().any(|reference| {
        !reference.cached
            && reference.oid == oid
            && ctx.current_branch == Some(reference.name.as_str())
    })
}

fn named_trunk_label(name: String, meta: Option<&CommitMeta>, ctx: &RenderContext<'_>) -> String {
    let Some(meta) = meta.filter(|_| ctx.verbosity.includes_metadata()) else {
        return name;
    };

    let count = trunk_count_placeholder(ctx.metadata_widths);
    if ctx.layout.is_columns() {
        let count_column = columns_count(&count, ctx.metadata_widths, ctx.colours, true);
        let mut label = format!("{count_column} {name}");
        if ctx.verbosity.includes_oid() {
            label.push(' ');
            label.push_str(&ctx.colours.metadata_oid(&meta.short_oid));
        }
        return label;
    }

    let age = trunk_metadata_age(meta, ctx.now_timestamp);
    let prefix = format_metadata_prefix(
        &age,
        &count,
        ctx.verbosity
            .includes_oid()
            .then_some(meta.short_oid.as_str()),
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
    let roots: Vec<_> = (0..layout.lane_count).collect();
    let lane_colours: Vec<_> = (0..layout.lane_count)
        .map(|index| layout.colour_offset + index)
        .collect();
    trunk_prefix_with_roots(
        layout,
        main_is_current,
        main_spine,
        &roots,
        &lane_colours,
        colours,
    )
}

pub(super) fn trunk_prefix_with_roots(
    layout: LaneRenderLayout,
    main_is_current: bool,
    main_spine: MainSpine,
    roots: &[usize],
    lane_colours: &[usize],
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
        let colour_index = lane_colours
            .first()
            .copied()
            .unwrap_or(layout.colour_offset);
        parts.push(colours.stack(colour_index, "──"));
    }
    let last = roots.last().copied().unwrap_or(0);
    for (index, &colour_index) in lane_colours.iter().enumerate() {
        let glyph = if index > last {
            "  "
        } else if index == last {
            "─┘"
        } else if roots.contains(&index) {
            "─┴"
        } else {
            "──"
        };
        parts.push(colours.stack(colour_index, glyph));
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
            if let Some(label) = reference_label(&meta.oid, Some(meta), ctx) {
                return label;
            }
            let subject = ctx.colours.commit_title(&meta.subject);
            if !ctx.verbosity.includes_metadata() {
                return subject;
            }

            let count = trunk_count_placeholder(ctx.metadata_widths);
            if ctx.layout.is_columns() {
                let count_column = columns_count(&count, ctx.metadata_widths, ctx.colours, true);
                let mut label = if ctx.verbosity.includes_title() {
                    format!("{count_column} {subject}")
                } else {
                    count_column
                };
                if ctx.verbosity.includes_oid() {
                    label.push(' ');
                    label.push_str(&ctx.colours.metadata_oid(&meta.short_oid));
                }
                return label;
            }

            let age = trunk_metadata_age(meta, ctx.now_timestamp);
            let prefix = format_metadata_prefix(
                &age,
                &count,
                ctx.verbosity
                    .includes_oid()
                    .then_some(meta.short_oid.as_str()),
                ctx.metadata_widths,
                ctx.colours,
            );
            if ctx.verbosity.includes_title() {
                format!("{prefix} {subject}")
            } else {
                prefix
            }
        }
    }
}

/// The age string a trunk row contributes to the columns-layout gutter, or
/// `None` when metadata is suppressed or the main tip has no recorded commit.
pub(super) fn trunk_age(label: TrunkLabel<'_>, ctx: &RenderContext<'_>) -> Option<String> {
    if !ctx.verbosity.includes_metadata() {
        return None;
    }
    let meta = match label {
        TrunkLabel::Main => ctx.main_meta?,
        TrunkLabel::Commit(meta) => meta,
    };
    Some(trunk_metadata_age(meta, ctx.now_timestamp))
}

pub(super) fn render_main_tip(ctx: &RenderContext<'_>) -> RenderLine {
    let current_main = trunk_is_current(TrunkLabel::Main, ctx);
    let gutter = age_gutter(ctx, trunk_age(TrunkLabel::Main, ctx));
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
    let rendered = render_row(
        &gutter,
        &current_row_indicator(current_main, 0, ctx.colours),
        &line,
    );
    let oid = ctx
        .main_meta
        .filter(|_| ctx.layout.is_columns() && ctx.verbosity.includes_oid())
        .map(|meta| ctx.colours.metadata_oid(&meta.short_oid));
    match oid {
        Some(oid) => RenderLine::with_trailing_fixed_suffix(rendered, oid),
        None => RenderLine::plain(rendered),
    }
}

pub(super) fn render_top_spacer(
    ctx: &RenderContext<'_>,
    has_visible_rows_above_main: bool,
) -> RenderLine {
    if has_visible_rows_above_main {
        RenderLine::plain(String::new())
    } else {
        render_omitted_main_past(ctx)
    }
}

pub(super) fn render_collapsed_main_segment(
    commit_count: usize,
    ctx: &RenderContext<'_>,
) -> impl IntoIterator<Item = RenderLine> {
    let noun = if commit_count == 1 {
        "commit"
    } else {
        "commits"
    };
    let label = format!(
        "({commit_count} {noun} on {})",
        ctx.main_history_name.unwrap_or(ctx.main_name)
    );
    let gutter = age_gutter(ctx, None);
    [
        render_row(
            &gutter,
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{}",
                ctx.colours.stack(0, MAIN_SPINE_GLYPH)
            ),
        ),
        render_row(
            &gutter,
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{} {}",
                ctx.colours.stack(0, COLLAPSED_MAIN_GLYPH),
                ctx.colours.dim(&label)
            ),
        ),
        render_row(
            &gutter,
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{}",
                ctx.colours.stack(0, MAIN_SPINE_GLYPH)
            ),
        ),
    ]
    .map(RenderLine::plain)
}

pub(super) fn render_omitted_main_past(ctx: &RenderContext<'_>) -> RenderLine {
    let gutter = age_gutter(ctx, None);
    let line = format!(
        "{TREE_LEFT_PADDING}{}",
        ctx.colours.stack(0, COLLAPSED_MAIN_GLYPH)
    );
    RenderLine::plain(render_row(&gutter, " ", &line))
}

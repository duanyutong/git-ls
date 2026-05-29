use std::cmp::Ordering;
use std::io::IsTerminal;
use std::time::{SystemTime, UNIX_EPOCH};

use anstyle::{Ansi256Color, Style};

use crate::cli::{ColourMode, Palette, Verbosity};
use crate::model::{BranchAnnotation, BranchPoint, CommitMeta, Lane, LaneGroup};

const COLLAPSED_MAIN_GLYPH: &str = "⁝";
const MAIN_SPINE_GLYPH: &str = "│";
const MAIN_COMMIT_GLYPH: &str = "◇";
const CURRENT_MAIN_COMMIT_GLYPH: &str = "◆";
const ORPHANED_BRANCH_GLYPH: &str = "⦸";
const TREE_LEFT_PADDING: &str = "";
const BRANCH_LABEL_GAP: &str = " ";
const ANSI_METADATA_COUNT: u8 = 255;
const ANSI_MUTED_TEXT: u8 = 251;
const ANSI_ORPHANED_LABEL: u8 = 255;

pub(crate) struct Colours {
    enabled: bool,
    palette: &'static [u8],
}

impl Colours {
    pub(crate) fn new(mode: ColourMode, palette: Palette) -> Self {
        let enabled = match mode {
            ColourMode::Auto => std::io::stdout().is_terminal(),
            ColourMode::Always => true,
            ColourMode::Never => false,
        };
        Self {
            enabled,
            palette: palette.ansi_colours(),
        }
    }

    fn paint(&self, text: &str, style: Style) -> String {
        if !self.enabled || text.is_empty() {
            text.to_string()
        } else {
            format!("{style}{text}{style:#}")
        }
    }

    fn stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()]).on_default(),
        )
    }

    fn current_stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold()
                .underline(),
        )
    }

    fn current_indicator(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold(),
        )
    }

    fn dim(&self, text: &str) -> String {
        self.paint(text, Style::new().dimmed())
    }

    fn muted_text(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_MUTED_TEXT).on_default())
    }

    fn metadata_age(&self, text: &str) -> String {
        self.muted_text(text)
    }

    fn metadata_count(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_METADATA_COUNT).on_default())
    }

    fn metadata_oid(&self, text: &str) -> String {
        self.muted_text(text)
    }

    fn metadata_punctuation(&self, text: &str) -> String {
        self.muted_text(text)
    }

    fn commit_title(&self, text: &str) -> String {
        self.muted_text(text)
    }

    fn orphaned_name(&self, text: &str) -> String {
        self.metadata_count(text)
    }

    fn orphaned_glyph(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_METADATA_COUNT).on_default().bold())
    }

    fn orphaned_status(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_ORPHANED_LABEL).on_default().bold())
    }
}

fn marker_for(
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
) -> &'static str {
    if is_current_branch_point(point, current_branch) {
        "●"
    } else if head.is_some_and(|head| point.oid == head) {
        "◉"
    } else {
        "◯"
    }
}

fn is_current_branch_point(point: &BranchPoint, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
}

pub(crate) fn current_unix_timestamp() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

fn format_age(now_timestamp: i64, commit_timestamp: i64) -> String {
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
    age: usize,
    count: usize,
}

fn branch_metadata_columns(annotation: &BranchAnnotation, now_timestamp: i64) -> (String, String) {
    (
        format_age(now_timestamp, annotation.meta.timestamp),
        annotation.commit_count.to_string(),
    )
}

fn trunk_metadata_age(meta: &CommitMeta, now_timestamp: i64) -> String {
    format_age(now_timestamp, meta.timestamp)
}

fn trunk_count_placeholder(widths: MetadataWidths) -> String {
    "-".repeat(widths.count.max(1))
}

fn format_metadata_prefix(
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

fn display_names(
    point: &BranchPoint,
    current_branch: Option<&str>,
    colour_index: usize,
    now_timestamp: i64,
    verbosity: Verbosity,
    metadata_widths: MetadataWidths,
    colours: &Colours,
) -> String {
    let names = point
        .names
        .iter()
        .map(|name| {
            if current_branch.is_some_and(|branch| branch == name) {
                colours.current_stack(colour_index, name)
            } else {
                colours.stack(colour_index, name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let Some(annotation) = point
        .annotation
        .as_ref()
        .filter(|_| verbosity.includes_metadata())
    else {
        return names;
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
            "{prefix} {names} {}",
            colours.commit_title(&annotation.meta.subject)
        )
    } else {
        format!("{prefix} {names}")
    }
}

fn display_orphaned_names(
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

fn current_row_indicator(is_current: bool, colour_index: usize, colours: &Colours) -> String {
    if is_current {
        colours.current_indicator(colour_index, "▶")
    } else {
        " ".to_string()
    }
}

fn orphaned_row_indicator(is_current: bool, colours: &Colours) -> String {
    if is_current {
        colours.orphaned_glyph("▶")
    } else {
        " ".to_string()
    }
}

fn render_row(indicator: &str, content: &str) -> String {
    format!("{indicator} {content}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LaneRenderLayout {
    lane_count: usize,
    lane_field_width: usize,
    colour_offset: usize,
}

impl LaneRenderLayout {
    fn new(lane_count: usize, lane_field_width: usize, colour_offset: usize) -> Self {
        Self {
            lane_count,
            lane_field_width,
            colour_offset,
        }
    }

    fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    fn lane_padding(self) -> usize {
        self.lane_field_width.saturating_sub(self.lane_count)
    }
}

fn row_prefix(
    lane_index: usize,
    layout: LaneRenderLayout,
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
    main_spine: MainSpine,
    colours: &Colours,
) -> String {
    let mut slots = Vec::new();
    match main_spine {
        MainSpine::Hidden => {}
        MainSpine::Future => {
            slots.push(" ".to_string());
        }
        MainSpine::FutureLine => {
            slots.push(colours.stack(0, COLLAPSED_MAIN_GLYPH));
        }
        MainSpine::Connected => {
            slots.push(colours.stack(0, MAIN_SPINE_GLYPH));
        }
    }
    for _ in 0..layout.lane_padding() {
        slots.push(" ".to_string());
    }
    for index in 0..layout.lane_count {
        let colour_index = layout.colour_offset + index;
        match index.cmp(&lane_index) {
            Ordering::Less => slots.push(colours.stack(colour_index, "│")),
            Ordering::Equal => {
                slots.push(colours.stack(colour_index, marker_for(point, current_branch, head)));
            }
            Ordering::Greater => slots.push(" ".to_string()),
        }
    }
    format!("{TREE_LEFT_PADDING}{}", slots.join(" "))
}

fn main_is_current(main_name: &str, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| branch == main_name)
}

fn main_label(ctx: &RenderContext<'_>) -> String {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainSpine {
    Hidden,
    Future,
    FutureLine,
    Connected,
}

impl MainSpine {
    fn is_connected(self) -> bool {
        matches!(self, Self::Future | Self::FutureLine | Self::Connected)
    }
}

fn trunk_prefix(
    layout: LaneRenderLayout,
    main_is_current: bool,
    main_spine: MainSpine,
    colours: &Colours,
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
enum TrunkLabel<'a> {
    Main,
    Commit(&'a CommitMeta),
}

fn trunk_label(label: TrunkLabel<'_>, ctx: &RenderContext<'_>) -> String {
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

pub(crate) fn render_top_spacer(colours: &Colours, has_visible_rows_above_main: bool) -> String {
    if has_visible_rows_above_main {
        String::new()
    } else {
        render_omitted_main_past(colours)
    }
}

pub(crate) struct RenderContext<'a> {
    pub(crate) main_name: &'a str,
    pub(crate) main_meta: Option<&'a CommitMeta>,
    pub(crate) current_branch: Option<&'a str>,
    pub(crate) head: Option<&'a str>,
    pub(crate) now_timestamp: i64,
    pub(crate) verbosity: Verbosity,
    pub(crate) metadata_widths: MetadataWidths,
    pub(crate) colours: &'a Colours,
}

fn render_group(
    lanes: &[Lane],
    lane_field_width: usize,
    colour_offset: usize,
    ctx: &RenderContext<'_>,
    label: TrunkLabel<'_>,
    main_spine: MainSpine,
) -> Vec<String> {
    let lane_count = lanes.len();
    let layout = LaneRenderLayout::new(lane_count, lane_field_width, colour_offset);
    let point_count: usize = lanes.iter().map(|lane| lane.branch_points.len()).sum();
    let mut rendered_points = 0;
    let mut output = Vec::new();

    for (lane_index, lane) in lanes.iter().enumerate() {
        for point in &lane.branch_points {
            rendered_points += 1;
            let row_main_spine =
                if matches!(main_spine, MainSpine::Future) && rendered_points == point_count {
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

fn render_orphaned_group(lanes: &[Lane], ctx: &RenderContext<'_>) -> Vec<String> {
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

fn render_collapsed_main_segment(
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

pub(crate) fn render_omitted_main_past(colours: &Colours) -> String {
    let line = format!(
        "{TREE_LEFT_PADDING}{}",
        colours.stack(0, COLLAPSED_MAIN_GLYPH)
    );
    render_row(" ", &line)
}

fn connected_lane_field_width(groups: &[LaneGroup]) -> usize {
    groups
        .iter()
        .filter(|group| group.main_distance.is_some())
        .map(|group| group.lanes.len())
        .max()
        .unwrap_or(0)
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
        }
    }
    widths
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Palette, Verbosity};
    use crate::model::{BranchAnnotation, BranchPoint, CommitMeta, Lane, LaneGroup};
    use crate::test_support::{TEST_COMMIT_TIME, TEST_NOW};

    fn point(oid: &str, names: &[&str]) -> BranchPoint {
        BranchPoint {
            oid: oid.to_string(),
            names: names.iter().map(|name| (*name).to_string()).collect(),
            annotation: None,
        }
    }

    fn point_with_count(
        oid: &str,
        names: &[&str],
        commit_count: usize,
        subject: &str,
    ) -> BranchPoint {
        point_with_count_at(oid, names, commit_count, subject, TEST_COMMIT_TIME)
    }

    fn point_with_count_at(
        oid: &str,
        names: &[&str],
        commit_count: usize,
        subject: &str,
        timestamp: i64,
    ) -> BranchPoint {
        BranchPoint {
            oid: oid.to_string(),
            names: names.iter().map(|name| (*name).to_string()).collect(),
            annotation: Some(BranchAnnotation {
                meta: CommitMeta {
                    oid: oid.to_string(),
                    short_oid: oid.to_string(),
                    subject: subject.to_string(),
                    timestamp,
                },
                commit_count,
            }),
        }
    }

    fn meta(oid: &str, subject: &str) -> CommitMeta {
        CommitMeta {
            oid: oid.to_string(),
            short_oid: oid.to_string(),
            subject: subject.to_string(),
            timestamp: 0,
        }
    }

    fn test_colours(enabled: bool) -> Colours {
        Colours {
            enabled,
            palette: Palette::Classic.ansi_colours(),
        }
    }

    #[test]
    fn renders_markers_names_and_trunk() {
        let colours = test_colours(false);
        let lanes = vec![
            Lane {
                head_oid: "a".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("a", &["feature/one"])],
                head_timestamp: 1,
                contains_current: false,
            },
            Lane {
                head_oid: "b".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("b", &["feature/two"])],
                head_timestamp: 2,
                contains_current: true,
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/two"),
            head: Some("b"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_group(
            &lanes,
            lanes.len(),
            0,
            &ctx,
            TrunkLabel::Main,
            MainSpine::Future,
        );

        assert_eq!(
            output,
            vec![
                "    ◯   feature/one".to_string(),
                "▶ ⁝ │ ● feature/two".to_string(),
                "  ◇─┴─┘ main".to_string()
            ]
        );
    }

    #[test]
    fn renders_exactly_one_future_line_above_main_node() {
        let colours = test_colours(false);
        let lanes = vec![
            Lane {
                head_oid: "a".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("a", &["feature/one"])],
                head_timestamp: 1,
                contains_current: false,
            },
            Lane {
                head_oid: "b".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("b", &["feature/two"])],
                head_timestamp: 2,
                contains_current: true,
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/two"),
            head: Some("b"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_group(
            &lanes,
            lanes.len(),
            0,
            &ctx,
            TrunkLabel::Main,
            MainSpine::Future,
        );

        assert_eq!(
            output
                .iter()
                .filter(|line| line.contains(COLLAPSED_MAIN_GLYPH))
                .count(),
            1
        );
        assert_eq!(output[output.len() - 2], "▶ ⁝ │ ● feature/two");
        assert_eq!(output[output.len() - 1], "  ◇─┴─┘ main");
    }

    #[test]
    fn renders_branch_metadata_with_commit_count_for_multi_commit_branch() {
        let colours = test_colours(false);
        let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

        let label = display_names(
            &point,
            Some("other"),
            0,
            TEST_NOW,
            Verbosity::High,
            MetadataWidths::default(),
            &colours,
        );

        assert_eq!(label, "2m (3, branch-head) feature/topic finish topic");
    }

    #[test]
    fn renders_summary_branch_metadata_without_commit_title() {
        let colours = test_colours(false);
        let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

        let label = display_names(
            &point,
            Some("other"),
            0,
            TEST_NOW,
            Verbosity::Medium,
            MetadataWidths::default(),
            &colours,
        );

        assert_eq!(label, "2m (3, branch-head) feature/topic");
    }

    #[test]
    fn renders_single_main_based_lane_with_main_spine() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: true,
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/one"),
            head: Some("a"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_group(
            &lanes,
            lanes.len(),
            0,
            &ctx,
            TrunkLabel::Main,
            MainSpine::Future,
        );

        assert_eq!(
            output,
            vec!["▶ ⁝ ● feature/one".to_string(), "  ◇─┘ main".to_string()]
        );
    }

    #[test]
    fn renders_current_main_on_trunk_row() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_group(
            &lanes,
            lanes.len(),
            0,
            &ctx,
            TrunkLabel::Main,
            MainSpine::Future,
        );

        assert_eq!(
            output,
            vec!["  ⁝ ◯ feature/one".to_string(), "▶ ◆─┘ main".to_string()]
        );
    }

    #[test]
    fn renders_orphaned_lane_with_single_warning_marker() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "backup".to_string(),
            base_oid: None,
            branch_points: vec![point("backup", &["test-branch-name"])],
            head_timestamp: 1,
            contains_current: false,
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_orphaned_group(&lanes, &ctx);

        assert_eq!(
            output,
            vec!["  ⁝ ⦸ test-branch-name (orphaned)".to_string()]
        );
    }

    #[test]
    fn renders_orphaned_only_groups_around_main_tip() {
        let colours = test_colours(false);
        let groups = vec![LaneGroup {
            base_oid: None,
            base_meta: None,
            main_distance: None,
            lanes: vec![Lane {
                head_oid: "backup".to_string(),
                base_oid: None,
                branch_points: vec![point("backup", &["test-branch-name"])],
                head_timestamp: 1,
                contains_current: false,
            }],
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                "  ⁝".to_string(),
                "▶ ◆── main".to_string(),
                "  ⁝ ⦸ test-branch-name (orphaned)".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn renders_main_metadata_in_aligned_annotation_column() {
        let colours = test_colours(false);
        let main_meta = CommitMeta {
            oid: "main-oid".to_string(),
            short_oid: "main-oi".to_string(),
            subject: "main tip".to_string(),
            timestamp: TEST_COMMIT_TIME,
        };
        let groups = vec![LaneGroup {
            base_oid: None,
            base_meta: None,
            main_distance: None,
            lanes: vec![Lane {
                head_oid: "backup-oid".to_string(),
                base_oid: None,
                branch_points: vec![point_with_count_at(
                    "backup-oid",
                    &["backup"],
                    10,
                    "backup tip",
                    TEST_COMMIT_TIME,
                )],
                head_timestamp: TEST_COMMIT_TIME,
                contains_current: false,
            }],
        }];
        let metadata_widths =
            calculate_metadata_widths(&groups, Some(&main_meta), TEST_NOW, Verbosity::Medium);
        let ctx = RenderContext {
            main_name: "main",
            main_meta: Some(&main_meta),
            current_branch: Some("main"),
            head: Some("main-oid"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Medium,
            metadata_widths,
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                "  ⁝".to_string(),
                "▶ ◆── 2m (--, main-oi) main".to_string(),
                "  ⁝ ⦸ 2m (10, backup-oid) backup (orphaned)".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn renders_orphaned_groups_below_connected_stacks() {
        let colours = test_colours(false);
        let groups = vec![
            LaneGroup {
                base_oid: Some("main".to_string()),
                base_meta: None,
                main_distance: Some(0),
                lanes: vec![Lane {
                    head_oid: "feature".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature", &["feature/current"])],
                    head_timestamp: 2,
                    contains_current: true,
                }],
            },
            LaneGroup {
                base_oid: None,
                base_meta: None,
                main_distance: None,
                lanes: vec![
                    Lane {
                        head_oid: "orphan-a".to_string(),
                        base_oid: None,
                        branch_points: vec![point("orphan-a", &["orphan-A"])],
                        head_timestamp: 1,
                        contains_current: false,
                    },
                    Lane {
                        head_oid: "orphan-b".to_string(),
                        base_oid: None,
                        branch_points: vec![point("orphan-b", &["orphan-B"])],
                        head_timestamp: 1,
                        contains_current: false,
                    },
                ],
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/current"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                String::new(),
                "▶ ⁝ ● feature/current".to_string(),
                "  ◇─┘ main".to_string(),
                "  ⁝ ⦸ orphan-A (orphaned)".to_string(),
                "  ⁝ ⦸ orphan-B (orphaned)".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn renders_old_main_groups_with_collapsed_main_history() {
        let colours = test_colours(false);
        let groups = vec![
            LaneGroup {
                base_oid: Some("main".to_string()),
                base_meta: Some(meta("main", "main tip")),
                main_distance: Some(0),
                lanes: vec![
                    Lane {
                        head_oid: "feature-one".to_string(),
                        base_oid: Some("main".to_string()),
                        branch_points: vec![point("feature-one", &["feature/one"])],
                        head_timestamp: 4,
                        contains_current: false,
                    },
                    Lane {
                        head_oid: "feature-two".to_string(),
                        base_oid: Some("main".to_string()),
                        branch_points: vec![point("feature-two", &["feature/two"])],
                        head_timestamp: 3,
                        contains_current: false,
                    },
                    Lane {
                        head_oid: "feature-current".to_string(),
                        base_oid: Some("main".to_string()),
                        branch_points: vec![point("feature-current", &["feature/current"])],
                        head_timestamp: 2,
                        contains_current: false,
                    },
                ],
            },
            LaneGroup {
                base_oid: Some("old-main".to_string()),
                base_meta: Some(meta(
                    "old-main",
                    "chore: this is an old commit in main history",
                )),
                main_distance: Some(842),
                lanes: vec![Lane {
                    head_oid: "old-feature".to_string(),
                    base_oid: Some("old-main".to_string()),
                    branch_points: vec![point("old-feature", &["dyt/tgs_api"])],
                    head_timestamp: 1,
                    contains_current: true,
                }],
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("dyt/tgs_api"),
            head: Some("old-feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                String::new(),
                "    ◯     feature/one".to_string(),
                "    │ ◯   feature/two".to_string(),
                "  ⁝ │ │ ◯ feature/current".to_string(),
                "  ◇─┴─┴─┘ main".to_string(),
                "  │".to_string(),
                "  ⁝ (842 commits on main)".to_string(),
                "  │".to_string(),
                "▶ │     ● dyt/tgs_api".to_string(),
                "  ◇─────┘ chore: this is an old commit in main history".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn colours_text_when_enabled() {
        let colours = test_colours(true);

        assert_eq!(colours.stack(0, "x"), "\x1b[38;5;41mx\x1b[0m");
        assert_eq!(
            colours.current_stack(0, "x"),
            "\x1b[1m\x1b[4m\x1b[38;5;41mx\x1b[0m"
        );
        assert_eq!(
            colours.current_indicator(0, "x"),
            "\x1b[1m\x1b[38;5;41mx\x1b[0m"
        );
        assert_eq!(colours.dim("x"), "\x1b[2mx\x1b[0m");
        assert_eq!(colours.muted_text("x"), "\x1b[38;5;251mx\x1b[0m");
        assert_eq!(colours.metadata_age("x"), "\x1b[38;5;251mx\x1b[0m");
        assert_eq!(colours.metadata_count("x"), "\x1b[38;5;255mx\x1b[0m");
        assert_eq!(colours.metadata_oid("x"), "\x1b[38;5;251mx\x1b[0m");
        assert_eq!(colours.metadata_punctuation("x"), "\x1b[38;5;251mx\x1b[0m");
        assert_eq!(colours.commit_title("x"), "\x1b[38;5;251mx\x1b[0m");
        assert_eq!(colours.orphaned_name("x"), "\x1b[38;5;255mx\x1b[0m");
        assert_eq!(colours.orphaned_glyph("x"), "\x1b[1m\x1b[38;5;255mx\x1b[0m");
        assert_eq!(
            colours.orphaned_status("x"),
            "\x1b[1m\x1b[38;5;255mx\x1b[0m"
        );
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("main"),
            head: Some("main"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };
        assert_eq!(main_label(&ctx), "\x1b[1m\x1b[4m\x1b[38;5;41mmain\x1b[0m");
        assert_eq!(
            trunk_prefix(LaneRenderLayout::empty(), true, MainSpine::Hidden, &colours),
            "\x1b[38;5;41m◆\x1b[0m\x1b[38;5;41m──\x1b[0m"
        );
        let inactive_main_ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };
        assert_eq!(main_label(&inactive_main_ctx), "\x1b[38;5;41mmain\x1b[0m");
        assert_eq!(
            trunk_prefix(
                LaneRenderLayout::empty(),
                false,
                MainSpine::Hidden,
                &colours
            ),
            "\x1b[38;5;41m◇\x1b[0m\x1b[38;5;41m──\x1b[0m"
        );
    }

    #[test]
    fn main_reserves_first_palette_colour() {
        let colours = test_colours(true);
        let groups = vec![LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: None,
            main_distance: Some(0),
            lanes: vec![Lane {
                head_oid: "feature".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("feature", &["feature/one"])],
                head_timestamp: 1,
                contains_current: false,
            }],
        }];
        let ctx = RenderContext {
            main_name: "main",
            main_meta: None,
            current_branch: Some("feature/one"),
            head: Some("feature"),
            now_timestamp: TEST_NOW,
            verbosity: Verbosity::Low,
            metadata_widths: MetadataWidths::default(),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert!(output[1].contains("\x1b[38;5;203m●\x1b[0m"));
        assert!(output[2].contains("\x1b[38;5;41m◇\x1b[0m"));
        assert!(output[2].contains("\x1b[38;5;41mmain\x1b[0m"));
    }
}

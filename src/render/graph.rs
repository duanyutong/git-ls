use crate::model::BranchPoint;

use super::colours::Colours;

pub(super) const COLLAPSED_MAIN_GLYPH: &str = "⁝";
pub(super) const MAIN_SPINE_GLYPH: &str = "│";
pub(super) const MAIN_COMMIT_GLYPH: &str = "◇";
pub(super) const CURRENT_MAIN_COMMIT_GLYPH: &str = "◆";
pub(super) const ORPHANED_BRANCH_GLYPH: &str = "⦸";
pub(super) const REWRITTEN_COMMIT_GLYPH: &str = "✕";
pub(super) const TREE_LEFT_PADDING: &str = "";
pub(super) const BRANCH_LABEL_GAP: &str = " ";

pub(super) fn marker_for(
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

pub(super) fn is_current_branch_point(point: &BranchPoint, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
}

pub(super) fn current_row_indicator(
    is_current: bool,
    colour_index: usize,
    colours: &Colours,
) -> String {
    if is_current {
        colours.current_indicator(colour_index, "▶")
    } else {
        " ".to_string()
    }
}

pub(super) fn orphaned_row_indicator(is_current: bool, colours: &Colours) -> String {
    if is_current {
        colours.orphaned_glyph("▶")
    } else {
        " ".to_string()
    }
}

pub(super) fn render_row(gutter: &str, indicator: &str, content: &str) -> String {
    format!("{gutter}{indicator} {content}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LaneRenderLayout {
    pub(super) lane_count: usize,
    pub(super) lane_field_width: usize,
    pub(super) colour_offset: usize,
    visible: Option<usize>,
}

impl LaneRenderLayout {
    pub(super) const fn new(
        lane_count: usize,
        lane_field_width: usize,
        colour_offset: usize,
    ) -> Self {
        Self {
            lane_count,
            lane_field_width,
            colour_offset,
            visible: None,
        }
    }

    pub(super) const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    pub(super) const fn lane_padding(self) -> usize {
        self.lane_field_width.saturating_sub(self.lane_count)
    }

    pub(super) const fn with_width(mut self, width: Option<usize>) -> Self {
        self.visible = width;
        self
    }

    fn visible_lanes(self) -> usize {
        self.visible
            .map_or(self.lane_count, |width| self.lane_count.min(width / 2 + 1))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MainSpine {
    Hidden,
    Future,
    FutureLine,
    Connected,
}

impl MainSpine {
    pub(super) const fn is_connected(self) -> bool {
        matches!(self, Self::Future | Self::FutureLine | Self::Connected)
    }
}

pub(super) fn row_prefix_with_marker(
    lane_index: usize,
    layout: LaneRenderLayout,
    marker: &str,
    main_spine: MainSpine,
    active: &[bool],
    lane_colours: &[usize],
    colours: &Colours,
) -> String {
    let visible_count = layout.visible_lanes();
    crate::diagnostics::count("rendered_lane_cells", visible_count);
    let lanes = active
        .iter()
        .enumerate()
        .take(visible_count)
        .map(|(index, &visible)| {
            let glyph = if index == lane_index {
                marker
            } else if visible {
                "│"
            } else {
                " "
            };
            colours.stack(lane_colours[index], glyph)
        });
    let mut lanes: Vec<_> = lanes.collect();
    if active.len() > visible_count {
        lanes.push(" ".repeat((active.len() - visible_count) * 2 - 1));
    }
    graph_prefix(layout, main_spine, lanes, colours)
}

pub(super) fn junction_prefix(
    layout: LaneRenderLayout,
    main_spine: MainSpine,
    children: &[usize],
    active: &[bool],
    lane_colours: &[usize],
    colours: &Colours,
) -> String {
    let first = children.iter().min().copied().unwrap_or(0);
    let last = children.iter().max().copied().unwrap_or(0);
    let mut lanes = Vec::new();
    let visible_count = layout.visible_lanes();
    for (index, &visible) in active.iter().enumerate().take(visible_count) {
        let glyph = if index == first {
            "├"
        } else if index == last {
            "┘"
        } else if children.contains(&index) {
            "┴"
        } else if index > first && index < last {
            "─"
        } else if visible {
            "│"
        } else {
            " "
        };
        let gap = if index > first && index <= last {
            "─"
        } else {
            " "
        };
        let slot = if index == 0 {
            glyph.to_string()
        } else {
            format!("{gap}{glyph}")
        };
        lanes.push(colours.stack(lane_colours[index], &slot));
    }
    if active.len() > visible_count {
        lanes.push(" ".repeat((active.len() - visible_count) * 2));
    }
    graph_prefix(layout, main_spine, vec![lanes.concat()], colours)
}

fn graph_prefix(
    layout: LaneRenderLayout,
    main_spine: MainSpine,
    lanes: Vec<String>,
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
    slots.extend(lanes);
    format!("{TREE_LEFT_PADDING}{}", slots.join(" "))
}

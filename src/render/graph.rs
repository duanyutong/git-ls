use std::cmp::Ordering;

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
}

impl LaneRenderLayout {
    pub(super) fn new(lane_count: usize, lane_field_width: usize, colour_offset: usize) -> Self {
        Self {
            lane_count,
            lane_field_width,
            colour_offset,
        }
    }

    pub(super) fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    pub(super) fn lane_padding(self) -> usize {
        self.lane_field_width.saturating_sub(self.lane_count)
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
    pub(super) fn is_connected(self) -> bool {
        matches!(self, Self::Future | Self::FutureLine | Self::Connected)
    }
}

pub(super) fn row_prefix(
    lane_index: usize,
    layout: LaneRenderLayout,
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
    main_spine: MainSpine,
    colours: &Colours,
) -> String {
    row_prefix_with_marker(
        lane_index,
        layout,
        marker_for(point, current_branch, head),
        main_spine,
        colours,
    )
}

pub(super) fn row_prefix_with_marker(
    lane_index: usize,
    layout: LaneRenderLayout,
    marker: &str,
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
                slots.push(colours.stack(colour_index, marker));
            }
            Ordering::Greater => slots.push(" ".to_string()),
        }
    }
    format!("{TREE_LEFT_PADDING}{}", slots.join(" "))
}

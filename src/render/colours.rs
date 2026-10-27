use anstyle::{Ansi256Color, AnsiColor, Style};

use crate::model::LaneGroup;

mod assignment;

const ANSI_METADATA_COUNT: u8 = 255;
const ANSI_ORPHANED_LABEL: u8 = 255;

pub(crate) struct Colours {
    enabled: bool,
    assignment: Vec<usize>,
}

impl Colours {
    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            assignment: Vec::new(),
        }
    }

    pub(super) fn for_groups(&self, groups: &[LaneGroup]) -> Self {
        Self {
            enabled: self.enabled,
            assignment: if self.enabled {
                assignment::assign(groups)
            } else {
                Vec::new()
            },
        }
    }

    fn ansi_colour(&self, index: usize) -> Ansi256Color {
        Ansi256Color(assignment::ansi_colour(
            self.assignment.get(index).copied().unwrap_or(index),
        ))
    }

    pub(super) fn paint(&self, text: &str, style: Style) -> String {
        if !self.enabled || text.is_empty() {
            text.to_string()
        } else {
            format!("{style}{text}{style:#}")
        }
    }

    pub(super) fn stack(&self, index: usize, text: &str) -> String {
        self.paint(text, self.ansi_colour(index).on_default())
    }

    pub(super) fn current_stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            self.ansi_colour(index).on_default().bold().underline(),
        )
    }

    pub(super) fn current_indicator(&self, index: usize, text: &str) -> String {
        self.paint(text, self.ansi_colour(index).on_default().bold())
    }

    pub(super) fn dim(&self, text: &str) -> String {
        self.paint(text, Style::new().dimmed())
    }

    pub(super) fn muted_text(&self, text: &str) -> String {
        self.paint(text, Style::new())
    }

    pub(super) fn metadata_age(&self, text: &str) -> String {
        self.muted_text(text)
    }

    pub(super) fn metadata_count(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_METADATA_COUNT).on_default())
    }

    pub(super) fn metadata_oid(&self, text: &str) -> String {
        self.muted_text(text)
    }

    pub(super) fn metadata_punctuation(&self, text: &str) -> String {
        self.muted_text(text)
    }

    pub(super) fn commit_title(&self, text: &str) -> String {
        self.muted_text(text)
    }

    pub(super) fn diff_added(&self, text: &str) -> String {
        self.paint(text, AnsiColor::Green.on_default())
    }

    pub(super) fn diff_deleted(&self, text: &str) -> String {
        self.paint(text, AnsiColor::Red.on_default())
    }

    pub(super) fn orphaned_name(&self, text: &str) -> String {
        self.metadata_count(text)
    }

    pub(super) fn orphaned_glyph(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_METADATA_COUNT).on_default().bold())
    }

    pub(super) fn orphaned_status(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_ORPHANED_LABEL).on_default().bold())
    }
}

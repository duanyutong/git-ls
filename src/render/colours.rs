use anstyle::{Ansi256Color, Style};

use crate::cli::Palette;

const ANSI_METADATA_COUNT: u8 = 255;
const ANSI_MUTED_TEXT: u8 = 251;
const ANSI_ORPHANED_LABEL: u8 = 255;

pub(crate) struct Colours {
    pub(super) enabled: bool,
    pub(super) palette: &'static [u8],
}

impl Colours {
    pub(crate) fn new(enabled: bool, palette: Palette) -> Self {
        Self {
            enabled,
            palette: palette.ansi_colours(),
        }
    }

    pub(super) fn paint(&self, text: &str, style: Style) -> String {
        if !self.enabled || text.is_empty() {
            text.to_string()
        } else {
            format!("{style}{text}{style:#}")
        }
    }

    pub(super) fn stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()]).on_default(),
        )
    }

    pub(super) fn current_stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold()
                .underline(),
        )
    }

    pub(super) fn current_indicator(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold(),
        )
    }

    pub(super) fn dim(&self, text: &str) -> String {
        self.paint(text, Style::new().dimmed())
    }

    pub(super) fn muted_text(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(ANSI_MUTED_TEXT).on_default())
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

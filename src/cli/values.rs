use std::fmt;

use clap::ValueEnum;

const OKABE_PALETTE: [u8; 7] = [214, 45, 35, 220, 32, 202, 176];
const TABLEAU_PALETTE: [u8; 10] = [67, 215, 167, 73, 71, 221, 139, 217, 137, 249];
const DARK2_PALETTE: [u8; 8] = [36, 166, 98, 162, 70, 178, 136, 242];
const SET1_PALETTE: [u8; 9] = [196, 33, 34, 127, 208, 226, 130, 211, 246];
const SET2_PALETTE: [u8; 8] = [79, 209, 110, 176, 149, 220, 180, 249];
const PAIRED_PALETTE: [u8; 12] = [153, 32, 150, 34, 210, 196, 215, 208, 183, 97, 228, 130];
const BOLD_PALETTE: [u8; 12] = [91, 36, 67, 220, 168, 107, 208, 30, 163, 209, 60, 145];
const VIVID_PALETTE: [u8; 12] = [208, 61, 73, 149, 170, 30, 178, 32, 97, 203, 162, 145];
const TOL_PALETTE: [u8; 7] = [67, 203, 29, 179, 81, 125, 250];
const CLASSIC_PALETTE: [u8; 7] = [41, 203, 39, 220, 177, 33, 214];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verbosity {
    Low,
    Medium,
    High,
}

impl Verbosity {
    pub(crate) fn from_count(count: u8) -> Self {
        match count {
            0 => Self::Low,
            1 => Self::Medium,
            _ => Self::High,
        }
    }

    pub(crate) fn try_from_config(value: u8) -> Option<Self> {
        match value {
            0..=2 => Some(Self::from_count(value)),
            _ => None,
        }
    }

    pub(crate) fn includes_metadata(self) -> bool {
        !matches!(self, Self::Low)
    }

    pub(crate) fn includes_oid(self) -> bool {
        matches!(self, Self::High)
    }

    pub(crate) fn includes_title(self) -> bool {
        matches!(self, Self::High)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ColourMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum Palette {
    #[value(name = "okabe")]
    Okabe,
    Tableau,
    Dark2,
    Set1,
    Set2,
    Paired,
    Bold,
    Vivid,
    Tol,
    #[default]
    Classic,
}

impl Palette {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Okabe => "okabe",
            Self::Tableau => "tableau",
            Self::Dark2 => "dark2",
            Self::Set1 => "set1",
            Self::Set2 => "set2",
            Self::Paired => "paired",
            Self::Bold => "bold",
            Self::Vivid => "vivid",
            Self::Tol => "tol",
            Self::Classic => "classic",
        }
    }

    pub(crate) fn ansi_colours(self) -> &'static [u8] {
        match self {
            Self::Okabe => &OKABE_PALETTE,
            Self::Tableau => &TABLEAU_PALETTE,
            Self::Dark2 => &DARK2_PALETTE,
            Self::Set1 => &SET1_PALETTE,
            Self::Set2 => &SET2_PALETTE,
            Self::Paired => &PAIRED_PALETTE,
            Self::Bold => &BOLD_PALETTE,
            Self::Vivid => &VIVID_PALETTE,
            Self::Tol => &TOL_PALETTE,
            Self::Classic => &CLASSIC_PALETTE,
        }
    }
}

impl fmt::Display for Palette {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum Layout {
    #[default]
    Inline,
    Columns,
}

impl Layout {
    pub(crate) fn is_columns(self) -> bool {
        matches!(self, Self::Columns)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Order {
    Newest,
    Oldest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Backend {
    Gix,
    Shell,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_rejects_out_of_range_config_values() {
        assert_eq!(Verbosity::try_from_config(3), None);
        assert_eq!(Verbosity::from_count(7), Verbosity::High);
        assert!(Verbosity::Medium.includes_metadata());
        assert!(!Verbosity::Medium.includes_oid());
        assert!(!Verbosity::Medium.includes_title());
        assert!(Verbosity::High.includes_oid());
    }

    #[test]
    fn layout_default_is_inline_and_columns_is_detected() {
        assert_eq!(Layout::default(), Layout::Inline);
        assert!(!Layout::Inline.is_columns());
        assert!(Layout::Columns.is_columns());
    }

    #[test]
    fn palette_names_and_display_text_cover_every_variant() {
        let names = [
            (Palette::Okabe, "okabe"),
            (Palette::Tableau, "tableau"),
            (Palette::Dark2, "dark2"),
            (Palette::Set1, "set1"),
            (Palette::Set2, "set2"),
            (Palette::Paired, "paired"),
            (Palette::Bold, "bold"),
            (Palette::Vivid, "vivid"),
            (Palette::Tol, "tol"),
            (Palette::Classic, "classic"),
        ];

        for (palette, name) in names {
            assert_eq!(palette.name(), name);
            assert_eq!(palette.to_string(), name);
        }
        assert_eq!(Palette::default(), Palette::Classic);
    }

    #[test]
    fn palette_ansi_tables_cover_every_variant() {
        assert_eq!(Palette::Okabe.ansi_colours(), OKABE_PALETTE);
        assert_eq!(Palette::Tableau.ansi_colours(), TABLEAU_PALETTE);
        assert_eq!(Palette::Dark2.ansi_colours(), DARK2_PALETTE);
        assert_eq!(Palette::Set1.ansi_colours(), SET1_PALETTE);
        assert_eq!(Palette::Set2.ansi_colours(), SET2_PALETTE);
        assert_eq!(Palette::Paired.ansi_colours(), PAIRED_PALETTE);
        assert_eq!(Palette::Bold.ansi_colours(), BOLD_PALETTE);
        assert_eq!(Palette::Vivid.ansi_colours(), VIVID_PALETTE);
        assert_eq!(Palette::Tol.ansi_colours(), TOL_PALETTE);
        assert_eq!(Palette::Classic.ansi_colours(), CLASSIC_PALETTE);
    }
}

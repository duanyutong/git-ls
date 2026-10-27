use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verbosity {
    Low,
    Medium,
    High,
}

impl Verbosity {
    pub(crate) const fn from_count(count: u8) -> Self {
        match count {
            0 => Self::Low,
            1 => Self::Medium,
            _ => Self::High,
        }
    }

    pub(crate) const fn try_from_config(value: u8) -> Option<Self> {
        match value {
            0..=2 => Some(Self::from_count(value)),
            _ => None,
        }
    }

    pub(crate) const fn includes_metadata(self) -> bool {
        !matches!(self, Self::Low)
    }

    pub(crate) const fn includes_oid(self) -> bool {
        matches!(self, Self::High)
    }

    pub(crate) const fn includes_title(self) -> bool {
        matches!(self, Self::High)
    }

    pub(crate) const fn includes_diff_stat(self) -> bool {
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
pub(crate) enum Layout {
    #[default]
    Inline,
    Columns,
}

impl Layout {
    pub(crate) const fn is_columns(self) -> bool {
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
#[path = "tests/values.rs"]
mod tests;

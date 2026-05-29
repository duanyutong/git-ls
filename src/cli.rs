use std::fmt;

use clap::{ArgAction, Parser, ValueEnum};

use crate::backend::{GitCommand, non_empty};
use crate::error::{GitLsError, Result};

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
const DEFAULT_PALETTE: Palette = Palette::Classic;
const DEFAULT_VERBOSITY: Verbosity = Verbosity::Medium;
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git ",
    env!("VERGEN_GIT_SHA"),
    ", dirty=",
    env!("VERGEN_GIT_DIRTY"),
    ", target=",
    env!("VERGEN_CARGO_TARGET_TRIPLE"),
    ", rustc=",
    env!("VERGEN_RUSTC_SEMVER"),
    ", built=",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")"
);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
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

impl Default for Palette {
    fn default() -> Self {
        DEFAULT_PALETTE
    }
}

impl fmt::Display for Palette {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
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

#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "git ls",
    about = "Render git-branchless draft branches as coloured stack lanes.",
    version = VERSION
)]
pub(crate) struct Args {
    #[arg(default_value = "draft()", value_name = "REVSET")]
    pub(crate) revset: String,

    #[arg(long)]
    pub(crate) hidden: bool,

    #[arg(short, long, action = ArgAction::Count)]
    pub(crate) verbose: u8,

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) backend: Option<Backend>,

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) order: Option<Order>,

    #[arg(long = "color", alias = "colour", value_enum, value_name = "VALUE")]
    pub(crate) colour_mode: Option<ColourMode>,

    #[arg(short = 'p', long, value_enum, value_name = "VALUE")]
    pub(crate) palette: Option<Palette>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectiveArgs {
    pub(crate) revset: String,
    pub(crate) hidden: bool,
    pub(crate) verbosity: Verbosity,
    pub(crate) backend: Backend,
    pub(crate) order: Order,
    pub(crate) colour_mode: ColourMode,
    pub(crate) palette: Palette,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GitLsConfig {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) backend: Option<Backend>,
    pub(crate) palette: Option<Palette>,
}

impl Args {
    pub(crate) fn resolve(&self, config: &GitLsConfig) -> EffectiveArgs {
        EffectiveArgs {
            revset: self.revset.clone(),
            hidden: self.hidden,
            verbosity: if self.verbose == 0 {
                config.verbosity.unwrap_or(DEFAULT_VERBOSITY)
            } else {
                Verbosity::from_count(self.verbose)
            },
            backend: self.backend.or(config.backend).unwrap_or(Backend::Gix),
            order: self.order.unwrap_or(Order::Newest),
            colour_mode: self.colour_mode.unwrap_or(ColourMode::Auto),
            palette: self.palette.or(config.palette).unwrap_or(DEFAULT_PALETTE),
        }
    }
}

fn git_config_value<G: GitCommand + ?Sized>(git: &G, key: &'static str) -> Result<Option<String>> {
    Ok(non_empty(&git.run(&["config", "--get", key], true)?))
}

fn invalid_git_config(key: &'static str, value: &str, expected: &'static str) -> GitLsError {
    GitLsError::InvalidGitConfig {
        key,
        value: value.to_string(),
        expected,
    }
}

fn parse_backend_config(key: &'static str, value: &str) -> Result<Backend> {
    Backend::from_str(value, true).map_err(|_| invalid_git_config(key, value, "gix or shell"))
}

fn parse_palette_config(key: &'static str, value: &str) -> Result<Palette> {
    Palette::from_str(value, true).map_err(|_| {
        invalid_git_config(
            key,
            value,
            "okabe, tableau, dark2, set1, set2, paired, bold, vivid, tol, or classic",
        )
    })
}

fn parse_verbosity_config(key: &'static str, value: &str) -> Result<Verbosity> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .and_then(Verbosity::try_from_config)
        .ok_or_else(|| invalid_git_config(key, value, "0, 1, or 2"))
}

pub(crate) fn read_git_ls_config<G: GitCommand + ?Sized>(git: &G) -> Result<GitLsConfig> {
    const BACKEND_KEY: &str = "git-ls.backend";
    const PALETTE_KEY: &str = "git-ls.palette";
    const VERBOSITY_KEY: &str = "git-ls.verbosity";

    Ok(GitLsConfig {
        verbosity: git_config_value(git, VERBOSITY_KEY)?
            .as_deref()
            .map(|value| parse_verbosity_config(VERBOSITY_KEY, value))
            .transpose()?,
        backend: git_config_value(git, BACKEND_KEY)?
            .as_deref()
            .map(|value| parse_backend_config(BACKEND_KEY, value))
            .transpose()?,
        palette: git_config_value(git, PALETTE_KEY)?
            .as_deref()
            .map(|value| parse_palette_config(PALETTE_KEY, value))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GitLsError;
    use crate::test_support::MockGit;

    fn args_with(
        verbose: u8,
        backend: Option<Backend>,
        colour_mode: Option<ColourMode>,
        palette: Option<Palette>,
    ) -> Args {
        Args {
            revset: "draft()".to_string(),
            hidden: false,
            verbose,
            backend,
            order: None,
            colour_mode,
            palette,
        }
    }

    fn default_args() -> Args {
        args_with(0, None, None, None)
    }

    #[test]
    fn parses_integer_verbosity_config_only() {
        assert_eq!(
            parse_verbosity_config("git-ls.verbosity", "0").unwrap(),
            Verbosity::Low
        );
        assert_eq!(
            parse_verbosity_config("git-ls.verbosity", "1").unwrap(),
            Verbosity::Medium
        );
        assert_eq!(
            parse_verbosity_config("git-ls.verbosity", "2").unwrap(),
            Verbosity::High
        );

        assert!(matches!(
            parse_verbosity_config("git-ls.verbosity", "full"),
            Err(GitLsError::InvalidGitConfig { .. })
        ));
    }

    #[test]
    fn reads_git_ls_config_defaults() {
        let git = MockGit::default()
            .with(&["config", "--get", "git-ls.verbosity"], "2")
            .with(&["config", "--get", "git-ls.backend"], "shell")
            .with(&["config", "--get", "git-ls.palette"], "okabe");

        let config = read_git_ls_config(&git).unwrap();
        let args = default_args().resolve(&config);

        assert_eq!(config.verbosity, Some(Verbosity::High));
        assert_eq!(config.backend, Some(Backend::Shell));
        assert_eq!(config.palette, Some(Palette::Okabe));
        assert_eq!(args.verbosity, Verbosity::High);
        assert_eq!(args.backend, Backend::Shell);
        assert_eq!(args.palette, Palette::Okabe);
    }

    #[test]
    fn uses_medium_verbosity_by_default() {
        let args = default_args().resolve(&GitLsConfig::default());

        assert_eq!(args.verbosity, Verbosity::Medium);
    }

    #[test]
    fn explicit_cli_options_override_git_ls_config() {
        let config = GitLsConfig {
            verbosity: Some(Verbosity::High),
            backend: Some(Backend::Shell),
            palette: Some(Palette::Okabe),
        };
        let args = args_with(1, Some(Backend::Gix), None, Some(Palette::Classic)).resolve(&config);

        assert_eq!(args.verbosity, Verbosity::Medium);
        assert_eq!(args.backend, Backend::Gix);
        assert_eq!(args.palette, Palette::Classic);
    }
}

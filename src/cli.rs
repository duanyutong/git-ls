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
const DEFAULT_REVSET: &str = "draft()";
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
        DEFAULT_RUNTIME_OPTIONS.palette
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeDefaults {
    verbosity: Verbosity,
    backend: Backend,
    order: Order,
    colour_mode: ColourMode,
    palette: Palette,
}

const DEFAULT_RUNTIME_OPTIONS: RuntimeDefaults = RuntimeDefaults {
    verbosity: Verbosity::Medium,
    backend: Backend::Gix,
    order: Order::Newest,
    colour_mode: ColourMode::Auto,
    palette: Palette::Classic,
};

#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "git ls",
    about = "Render git-branchless draft branches as coloured stack lanes.",
    version = VERSION
)]
pub(crate) struct Args {
    #[arg(default_value = DEFAULT_REVSET, value_name = "REVSET")]
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
pub(crate) struct RuntimeOptions {
    pub(crate) revset: String,
    pub(crate) hidden: bool,
    pub(crate) verbosity: Verbosity,
    pub(crate) backend: Backend,
    pub(crate) order: Order,
    pub(crate) colour_mode: ColourMode,
    pub(crate) palette: Palette,
}

impl RuntimeOptions {
    fn resolve(args: &Args, config: &GitLsConfig) -> Self {
        let defaults = DEFAULT_RUNTIME_OPTIONS;

        Self {
            revset: args.revset.clone(),
            hidden: args.hidden,
            verbosity: args
                .explicit_verbosity()
                .or(config.verbosity)
                .unwrap_or(defaults.verbosity),
            backend: args.backend.or(config.backend).unwrap_or(defaults.backend),
            order: args.order.unwrap_or(defaults.order),
            colour_mode: args.colour_mode.unwrap_or(defaults.colour_mode),
            palette: args.palette.or(config.palette).unwrap_or(defaults.palette),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GitLsConfig {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) backend: Option<Backend>,
    pub(crate) palette: Option<Palette>,
}

impl Args {
    pub(crate) fn resolve(&self, config: &GitLsConfig) -> RuntimeOptions {
        RuntimeOptions::resolve(self, config)
    }

    fn explicit_verbosity(&self) -> Option<Verbosity> {
        (self.verbose > 0).then(|| Verbosity::from_count(self.verbose))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigKey {
    Verbosity,
    Backend,
    Palette,
}

impl ConfigKey {
    const LOOKUP_ORDER: [Self; 3] = [Self::Verbosity, Self::Backend, Self::Palette];

    fn name(self) -> &'static str {
        match self {
            Self::Verbosity => "git-ls.verbosity",
            Self::Backend => "git-ls.backend",
            Self::Palette => "git-ls.palette",
        }
    }

    fn expected(self) -> &'static str {
        match self {
            Self::Verbosity => "0, 1, or 2",
            Self::Backend => "gix or shell",
            Self::Palette => {
                "okabe, tableau, dark2, set1, set2, paired, bold, vivid, tol, or classic"
            }
        }
    }
}

fn git_config_value<G: GitCommand + ?Sized>(git: &G, key: ConfigKey) -> Result<Option<String>> {
    Ok(non_empty(&git.run(&["config", "--get", key.name()], true)?))
}

fn invalid_git_config(key: ConfigKey, value: &str) -> GitLsError {
    GitLsError::InvalidGitConfig {
        key: key.name(),
        value: value.to_string(),
        expected: key.expected(),
    }
}

fn parse_backend_config(key: ConfigKey, value: &str) -> Result<Backend> {
    Backend::from_str(value, true).map_err(|_| invalid_git_config(key, value))
}

fn parse_palette_config(key: ConfigKey, value: &str) -> Result<Palette> {
    Palette::from_str(value, true).map_err(|_| invalid_git_config(key, value))
}

fn parse_verbosity_config(key: ConfigKey, value: &str) -> Result<Verbosity> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .and_then(Verbosity::try_from_config)
        .ok_or_else(|| invalid_git_config(key, value))
}

impl GitLsConfig {
    fn set_from_git_config(&mut self, key: ConfigKey, value: &str) -> Result<()> {
        match key {
            ConfigKey::Verbosity => self.verbosity = Some(parse_verbosity_config(key, value)?),
            ConfigKey::Backend => self.backend = Some(parse_backend_config(key, value)?),
            ConfigKey::Palette => self.palette = Some(parse_palette_config(key, value)?),
        }
        Ok(())
    }
}

pub(crate) fn read_git_ls_config<G: GitCommand + ?Sized>(git: &G) -> Result<GitLsConfig> {
    let mut config = GitLsConfig::default();
    for key in ConfigKey::LOOKUP_ORDER {
        if let Some(value) = git_config_value(git, key)? {
            config.set_from_git_config(key, &value)?;
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GitLsError;
    use crate::test_support::MockGit;

    fn args_with(
        verbose: u8,
        backend: Option<Backend>,
        order: Option<Order>,
        colour_mode: Option<ColourMode>,
        palette: Option<Palette>,
    ) -> Args {
        Args {
            revset: DEFAULT_REVSET.to_string(),
            hidden: false,
            verbose,
            backend,
            order,
            colour_mode,
            palette,
        }
    }

    fn default_args() -> Args {
        args_with(0, None, None, None, None)
    }

    fn config_call(key: ConfigKey) -> Vec<String> {
        ["config", "--get", key.name()]
            .iter()
            .map(|value| (*value).to_string())
            .collect()
    }

    #[test]
    fn parses_integer_verbosity_config_only() {
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "0").unwrap(),
            Verbosity::Low
        );
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "1").unwrap(),
            Verbosity::Medium
        );
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "2").unwrap(),
            Verbosity::High
        );

        let error = parse_verbosity_config(ConfigKey::Verbosity, "full").unwrap_err();
        assert!(matches!(
            error,
            GitLsError::InvalidGitConfig { key, expected, .. }
                if key == ConfigKey::Verbosity.name()
                    && expected == ConfigKey::Verbosity.expected()
        ));
    }

    #[test]
    fn reads_git_ls_config_in_declared_lookup_order() {
        let git = MockGit::default();

        let config = read_git_ls_config(&git).unwrap();

        assert_eq!(config, GitLsConfig::default());
        assert_eq!(
            git.calls(),
            ConfigKey::LOOKUP_ORDER
                .into_iter()
                .map(config_call)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reads_git_ls_config_values() {
        let git = MockGit::default()
            .with(&["config", "--get", ConfigKey::Verbosity.name()], "2")
            .with(&["config", "--get", ConfigKey::Backend.name()], "shell")
            .with(&["config", "--get", ConfigKey::Palette.name()], "okabe");

        let config = read_git_ls_config(&git).unwrap();

        assert_eq!(config.verbosity, Some(Verbosity::High));
        assert_eq!(config.backend, Some(Backend::Shell));
        assert_eq!(config.palette, Some(Palette::Okabe));
    }

    #[test]
    fn runtime_options_use_central_defaults_without_cli_or_config() {
        let args = default_args().resolve(&GitLsConfig::default());

        assert_eq!(
            args,
            RuntimeOptions {
                revset: DEFAULT_REVSET.to_string(),
                hidden: false,
                verbosity: DEFAULT_RUNTIME_OPTIONS.verbosity,
                backend: DEFAULT_RUNTIME_OPTIONS.backend,
                order: DEFAULT_RUNTIME_OPTIONS.order,
                colour_mode: DEFAULT_RUNTIME_OPTIONS.colour_mode,
                palette: DEFAULT_RUNTIME_OPTIONS.palette,
            }
        );
    }

    #[test]
    fn runtime_options_use_git_config_before_defaults() {
        let config = GitLsConfig {
            verbosity: Some(Verbosity::High),
            backend: Some(Backend::Shell),
            palette: Some(Palette::Okabe),
        };

        let args = default_args().resolve(&config);

        assert_eq!(args.verbosity, Verbosity::High);
        assert_eq!(args.backend, Backend::Shell);
        assert_eq!(args.palette, Palette::Okabe);
        assert_eq!(args.order, DEFAULT_RUNTIME_OPTIONS.order);
        assert_eq!(args.colour_mode, DEFAULT_RUNTIME_OPTIONS.colour_mode);
    }

    #[test]
    fn runtime_options_prefer_cli_over_git_config() {
        let config = GitLsConfig {
            verbosity: Some(Verbosity::High),
            backend: Some(Backend::Shell),
            palette: Some(Palette::Okabe),
        };
        let args = args_with(
            1,
            Some(Backend::Gix),
            Some(Order::Oldest),
            Some(ColourMode::Never),
            Some(Palette::Classic),
        )
        .resolve(&config);

        assert_eq!(args.verbosity, Verbosity::Medium);
        assert_eq!(args.backend, Backend::Gix);
        assert_eq!(args.order, Order::Oldest);
        assert_eq!(args.colour_mode, ColourMode::Never);
        assert_eq!(args.palette, Palette::Classic);
    }
}

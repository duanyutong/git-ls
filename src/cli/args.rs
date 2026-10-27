use clap::{ArgAction, Parser};

use super::defaults::DEFAULT_REVSET;
use super::values::{Backend, ColourMode, Layout, Order};

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

// Independent command-line switches are represented directly by clap.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "git ls",
    about = "Render local Git branch stacks as coloured lanes.",
    version = VERSION
)]
pub(crate) struct Args {
    #[arg(default_value = DEFAULT_REVSET, value_name = "REVSET")]
    pub(crate) revset: String,

    #[arg(long)]
    pub(crate) hidden: bool,

    #[arg(long)]
    pub(crate) debug: bool,

    /// Bypass all persistent cache reads, writes, and maintenance.
    #[arg(long, conflicts_with = "refresh_cache")]
    pub(crate) no_cache: bool,

    /// Recompute the analysis and replace eligible cached results.
    #[arg(long)]
    pub(crate) refresh_cache: bool,

    /// Remove this repository's git-ls cache before rendering without caching.
    #[arg(long, conflicts_with = "refresh_cache")]
    pub(crate) clear_cache: bool,

    /// Suppress background maintenance of the application cache.
    #[arg(long)]
    pub(crate) no_cache_maintenance: bool,

    /// Select detail level 1 (-v) or 2 (-vv); level 2 includes diff stats unless overridden.
    #[arg(short, long, action = ArgAction::Count)]
    pub(crate) verbose: u8,

    /// Detail level: 0–2; level 2 includes diff stats unless overridden.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=2), value_name = "VALUE")]
    pub(crate) verbosity: Option<u8>,

    /// Show added/deleted line counts regardless of verbosity.
    #[arg(long, conflicts_with = "no_diff_stat")]
    pub(crate) diff_stat: bool,

    /// Hide added/deleted line counts regardless of verbosity.
    #[arg(long)]
    pub(crate) no_diff_stat: bool,

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) backend: Option<Backend>,

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) order: Option<Order>,

    #[arg(long = "color", alias = "colour", value_enum, value_name = "VALUE")]
    pub(crate) colour_mode: Option<ColourMode>,

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) layout: Option<Layout>,
}

use clap::{ArgAction, Parser};

use super::defaults::DEFAULT_REVSET;
use super::values::{Backend, ColourMode, Layout, Order, Palette};

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

    #[arg(long, value_enum, value_name = "VALUE")]
    pub(crate) layout: Option<Layout>,
}

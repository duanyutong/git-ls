use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};

use clap::Parser;

#[cfg(test)]
use crate::backend::GitCommand;
use crate::backend::{GitBackend, GixBackend, ProcessGit, get_commit_meta};
use crate::cli::{Args, Backend, EffectiveArgs, read_git_ls_config};
use crate::error::Result;
use crate::lanes::{build_lane_groups, build_lanes, ordered_lanes};
use crate::model::BuiltLanes;
use crate::render::{
    Colours, RenderContext, calculate_metadata_widths, current_unix_timestamp, render_lane_groups,
    render_main_tip, render_omitted_main_past, render_top_spacer,
};
use crate::terminal::{terminal_output_width, write_rendered_line};

pub(crate) fn parse_args_from<I, S>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let cli_args =
        std::iter::once(OsString::from("git ls")).chain(args.into_iter().map(Into::into));
    Args::try_parse_from(cli_args).map_err(Into::into)
}

pub(crate) fn execute<W, G>(
    args: &EffectiveArgs,
    git: &G,
    stdout: &mut W,
    terminal_width: Option<usize>,
) -> Result<()>
where
    W: Write,
    G: GitBackend + ?Sized,
{
    let colours = Colours::new(args.colour_mode, args.palette);

    let mut meta_cache = HashMap::new();
    let (lanes, main_oid, repository) = match build_lanes(git, args, &mut meta_cache)? {
        BuiltLanes::Empty {
            main_oid,
            repository,
        } => {
            let now_timestamp = current_unix_timestamp();
            let main_meta = if args.verbosity.includes_metadata() {
                Some(get_commit_meta(git, &main_oid, &mut meta_cache)?)
            } else {
                None
            };
            let metadata_widths =
                calculate_metadata_widths(&[], main_meta.as_ref(), now_timestamp, args.verbosity);
            let ctx = RenderContext {
                main_name: &repository.main_name,
                main_meta: main_meta.as_ref(),
                current_branch: repository.current_branch.as_deref(),
                head: repository.head.as_deref(),
                now_timestamp,
                verbosity: args.verbosity,
                metadata_widths,
                colours: &colours,
            };
            write_rendered_line(stdout, &render_top_spacer(&colours, false), terminal_width)?;
            write_rendered_line(stdout, &render_main_tip(&ctx), terminal_width)?;
            write_rendered_line(stdout, &render_omitted_main_past(&colours), terminal_width)?;
            return Ok(());
        }
        BuiltLanes::Populated {
            lanes,
            main_oid,
            repository,
        } => (lanes, main_oid, repository),
    };

    let lanes = ordered_lanes(lanes, args.order);
    let groups = build_lane_groups(git, lanes, &main_oid, &mut meta_cache)?;
    let now_timestamp = current_unix_timestamp();
    let main_meta = if args.verbosity.includes_metadata() {
        Some(get_commit_meta(git, &main_oid, &mut meta_cache)?)
    } else {
        None
    };
    let metadata_widths =
        calculate_metadata_widths(&groups, main_meta.as_ref(), now_timestamp, args.verbosity);

    let ctx = RenderContext {
        main_name: &repository.main_name,
        main_meta: main_meta.as_ref(),
        current_branch: repository.current_branch.as_deref(),
        head: repository.head.as_deref(),
        now_timestamp,
        verbosity: args.verbosity,
        metadata_widths,
        colours: &colours,
    };

    for line in render_lane_groups(&groups, &ctx) {
        write_rendered_line(stdout, &line, terminal_width)?;
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn run<I, S, W, G>(args: I, git: &G, stdout: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    G: GitBackend + GitCommand + ?Sized,
{
    let args = parse_args_from(args)?;
    let config = read_git_ls_config(git)?;
    let args = args.resolve(&config);
    execute(&args, git, stdout, None)
}

pub fn run_from_env() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let terminal_width = terminal_output_width();
    let args = parse_args_from(env::args().skip(1))?;
    let config_git = ProcessGit;
    let config = read_git_ls_config(&config_git)?;
    let args = args.resolve(&config);
    match args.backend {
        Backend::Gix => {
            let git = GixBackend::discover()?;
            execute(&args, &git, &mut stdout, terminal_width)
        }
        Backend::Shell => {
            let git = ProcessGit;
            execute(&args, &git, &mut stdout, terminal_width)
        }
    }
}

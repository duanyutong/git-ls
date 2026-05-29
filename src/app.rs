use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};

use clap::Parser;

#[cfg(test)]
use crate::backend::GitCommand;
use crate::backend::{CommitMetadataBackend, GitBackend, GixBackend, ProcessGit, get_commit_meta};
use crate::cli::{Args, Backend, RuntimeOptions, Verbosity, read_git_ls_config};
use crate::error::Result;
use crate::lanes::{build_lane_groups, build_lanes, ordered_lanes};
use crate::model::{BuiltLanes, CommitMeta, LaneGroup, RepositorySnapshot};
use crate::render::{
    Colours, RenderContext, calculate_metadata_widths,
    render_empty_selection as render_empty_lines, render_lane_groups,
};
use crate::terminal::{RenderEnvironment, write_rendered_line};

fn parse_args_from<I, S>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let cli_args =
        std::iter::once(OsString::from("git ls")).chain(args.into_iter().map(Into::into));
    Args::try_parse_from(cli_args).map_err(Into::into)
}

#[derive(Debug, Eq, PartialEq)]
struct RenderPlan {
    lines: Vec<String>,
}

impl RenderPlan {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }

    fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

struct RenderSession<'a> {
    repository: &'a RepositorySnapshot,
    main_meta: Option<&'a CommitMeta>,
    now_timestamp: i64,
    verbosity: Verbosity,
    colours: &'a Colours,
}

impl<'a> RenderSession<'a> {
    fn new(
        args: &RuntimeOptions,
        repository: &'a RepositorySnapshot,
        main_meta: Option<&'a CommitMeta>,
        environment: RenderEnvironment,
        colours: &'a Colours,
    ) -> Self {
        Self {
            repository,
            main_meta,
            now_timestamp: environment.now_timestamp(),
            verbosity: args.verbosity,
            colours,
        }
    }

    fn context(&self, groups: &[LaneGroup]) -> RenderContext<'_> {
        RenderContext::new(
            &self.repository.main_name,
            self.main_meta,
            self.repository.current_branch.as_deref(),
            self.repository.head.as_deref(),
            self.now_timestamp,
            self.verbosity,
            calculate_metadata_widths(groups, self.main_meta, self.now_timestamp, self.verbosity),
            self.colours,
        )
    }
}

fn main_metadata<G: CommitMetadataBackend + ?Sized>(
    args: &RuntimeOptions,
    git: &G,
    main_oid: &str,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<CommitMeta>> {
    if args.verbosity.includes_metadata() {
        Ok(Some(get_commit_meta(git, main_oid, meta_cache)?))
    } else {
        Ok(None)
    }
}

fn render_empty_selection(session: &RenderSession<'_>) -> RenderPlan {
    let context = session.context(&[]);
    RenderPlan::new(render_empty_lines(&context))
}

fn render_populated_selection(groups: &[LaneGroup], session: &RenderSession<'_>) -> RenderPlan {
    let context = session.context(groups);
    RenderPlan::new(render_lane_groups(groups, &context))
}

fn build_render_plan<G>(
    args: &RuntimeOptions,
    git: &G,
    environment: RenderEnvironment,
) -> Result<RenderPlan>
where
    G: GitBackend + ?Sized,
{
    let colours = Colours::new(environment.colour_enabled(args.colour_mode), args.palette);

    let mut meta_cache = HashMap::new();
    match build_lanes(git, args, &mut meta_cache)? {
        BuiltLanes::Empty {
            main_oid,
            repository,
        } => {
            let main_meta = main_metadata(args, git, &main_oid, &mut meta_cache)?;
            let session =
                RenderSession::new(args, &repository, main_meta.as_ref(), environment, &colours);
            Ok(render_empty_selection(&session))
        }
        BuiltLanes::Populated {
            lanes,
            main_oid,
            repository,
        } => {
            let lanes = ordered_lanes(lanes, args.order);
            let groups = build_lane_groups(git, lanes, &main_oid, args.order, &mut meta_cache)?;
            let main_meta = main_metadata(args, git, &main_oid, &mut meta_cache)?;
            let session =
                RenderSession::new(args, &repository, main_meta.as_ref(), environment, &colours);
            Ok(render_populated_selection(&groups, &session))
        }
    }
}

fn write_render_plan<W: Write>(
    stdout: &mut W,
    plan: &RenderPlan,
    environment: RenderEnvironment,
) -> Result<()> {
    for line in plan.lines() {
        write_rendered_line(stdout, line, environment)?;
    }
    Ok(())
}

fn execute<W, G>(
    args: &RuntimeOptions,
    git: &G,
    stdout: &mut W,
    environment: RenderEnvironment,
) -> Result<()>
where
    W: Write,
    G: GitBackend + ?Sized,
{
    let plan = build_render_plan(args, git, environment)?;
    write_render_plan(stdout, &plan, environment)
}

#[cfg(test)]
fn run<I, S, W, G>(args: I, git: &G, stdout: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    G: GitBackend + GitCommand + ?Sized,
{
    let args = parse_args_from(args)?;
    let config = read_git_ls_config(git)?;
    let args = args.resolve(&config);
    execute(
        &args,
        git,
        stdout,
        RenderEnvironment::new(crate::test_support::TEST_NOW, None, false),
    )
}

/// Executes the command-line entry point with process arguments and detected
/// terminal capabilities.
pub(crate) fn run_from_env() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let environment = RenderEnvironment::detect();
    let args = parse_args_from(env::args().skip(1))?;
    let config_git = ProcessGit;
    let config = read_git_ls_config(&config_git)?;
    let args = args.resolve(&config);
    match args.backend {
        Backend::Gix => {
            let git = GixBackend::discover()?;
            execute(&args, &git, &mut stdout, environment)
        }
        Backend::Shell => {
            let git = ProcessGit;
            execute(&args, &git, &mut stdout, environment)
        }
    }
}

#[cfg(test)]
mod tests;

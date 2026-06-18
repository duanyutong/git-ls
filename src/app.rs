use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;

use clap::Parser;

#[cfg(test)]
use crate::backend::GitCommand;
use crate::backend::{CommitMetadataBackend, GitBackend, get_commit_meta};
#[cfg(test)]
use crate::cli::read_git_ls_config;
use crate::cli::{Args, Layout, RuntimeOptions, Verbosity};
use crate::error::Result;
use crate::lanes::{build_lane_groups, build_lanes, ordered_lanes};
use crate::model::{BuiltLanes, CommitMeta, LaneGroup, RepositorySnapshot};
use crate::render::{
    Colours, RenderContext, RenderLine, calculate_metadata_widths,
    render_empty_selection as render_empty_lines, render_lane_groups,
};
use crate::terminal::{RenderEnvironment, write_rendered_line};

mod env;
pub use env::run_from_env;

fn debug_log(enabled: bool, message: std::fmt::Arguments<'_>) {
    if enabled {
        eprintln!("git-ls debug: {message}");
    }
}

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
    lines: Vec<RenderLine>,
}

impl RenderPlan {
    fn new<I>(lines: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<RenderLine>,
    {
        Self {
            lines: lines.into_iter().map(Into::into).collect(),
        }
    }

    fn lines(&self) -> impl Iterator<Item = &RenderLine> {
        self.lines.iter()
    }
}

struct RenderSession<'a> {
    repository: &'a RepositorySnapshot,
    main_meta: Option<&'a CommitMeta>,
    now_timestamp: i64,
    verbosity: Verbosity,
    layout: Layout,
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
            layout: args.layout,
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
        .with_layout(self.layout)
    }
}

fn main_metadata(
    args: &RuntimeOptions,
    git: &dyn CommitMetadataBackend,
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

fn build_render_plan(
    args: &RuntimeOptions,
    git: &dyn GitBackend,
    environment: RenderEnvironment,
) -> Result<RenderPlan> {
    debug_log(
        args.debug,
        format_args!(
            "render plan: backend={:?} revset={:?} hidden={} order={:?} layout={:?}",
            args.backend, args.revset, args.hidden, args.order, args.layout
        ),
    );
    let colours = Colours::new(environment.colour_enabled(args.colour_mode), args.palette);

    let mut meta_cache = HashMap::new();
    match build_lanes(git, args, &mut meta_cache)? {
        BuiltLanes::Empty {
            main_oid,
            repository,
        } => {
            debug_log(
                args.debug,
                format_args!(
                    "render plan: empty selection main={} main_branch={}",
                    main_oid, repository.main_name
                ),
            );
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
            debug_log(
                args.debug,
                format_args!(
                    "render plan: populated selection lanes={} main={} main_branch={}",
                    lanes.len(),
                    main_oid,
                    repository.main_name
                ),
            );
            let lanes = ordered_lanes(lanes, args.order);
            let groups = build_lane_groups(
                git,
                lanes,
                &main_oid,
                args.order,
                args.debug,
                &mut meta_cache,
            )?;
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

fn execute<W>(
    args: &RuntimeOptions,
    git: &dyn GitBackend,
    stdout: &mut W,
    environment: RenderEnvironment,
) -> Result<()>
where
    W: Write,
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
    G: GitBackend + GitCommand,
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

#[cfg(test)]
mod tests;

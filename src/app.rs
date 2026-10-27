use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;

use clap::Parser;

use crate::backend::{CommitMetadataBackend, GitBackend, get_commit_meta};
use crate::cli::{Args, Layout, RuntimeOptions, Verbosity};
use crate::error::Result;
use crate::lanes::{build_lane_groups, build_lanes, ordered_lanes, populate_diff_stats};
use crate::model::{BuiltLanes, CommitMeta, LaneGroup, RepositorySnapshot};
use crate::render::{
    Colours, RenderContext, RenderLine, calculate_metadata_widths,
    render_empty_selection as render_empty_lines, render_lane_groups,
};
use crate::terminal::{RenderEnvironment, write_rendered_line};

mod cached;
mod env;
pub use env::run_from_env;

fn debug_log(enabled: bool, message: std::fmt::Arguments<'_>) {
    if enabled {
        let _ = writeln!(std::io::stderr().lock(), "git-ls debug: {message}");
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
    const fn new(
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
        .with_diff_stats(groups)
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
    let _perf = crate::diagnostics::stage("layout");
    let context = session.context(&[]);
    RenderPlan::new(render_empty_lines(&context))
}

fn render_populated_selection(groups: &[LaneGroup], session: &RenderSession<'_>) -> RenderPlan {
    let _perf = crate::diagnostics::stage("layout");
    let context = session.context(groups);
    RenderPlan::new(render_lane_groups(groups, &context))
}

struct Analysis {
    groups: Vec<LaneGroup>,
    repository: RepositorySnapshot,
    main_meta: Option<CommitMeta>,
}

fn build_analysis(args: &RuntimeOptions, git: &dyn GitBackend) -> Result<Analysis> {
    let _perf = crate::diagnostics::stage("analysis");
    debug_log(
        args.debug,
        format_args!(
            "render plan: backend={:?} revset={:?} hidden={} order={:?} layout={:?} verbosity={:?}",
            args.backend, args.revset, args.hidden, args.order, args.layout, args.verbosity
        ),
    );
    let observer = crate::diagnostics::AnalysisDiagnostics::new(args.debug);
    let mut meta_cache = HashMap::new();
    let (lanes, main_oid, repository) = match build_lanes(git, args, &mut meta_cache, &observer)? {
        BuiltLanes::Empty {
            main_oid,
            repository,
        } => (Vec::new(), main_oid, repository),
        BuiltLanes::Populated {
            lanes,
            main_oid,
            repository,
        } => (lanes, main_oid, repository),
    };
    let lanes = ordered_lanes(lanes, args.order);
    if args.verbosity.includes_metadata() {
        let mut ids: Vec<_> = lanes
            .iter()
            .filter_map(|lane| lane.base_oid.as_deref())
            .collect();
        ids.push(&main_oid);
        ids.sort_unstable();
        ids.dedup();
        git.cache_commit_metas(&ids, &mut meta_cache)?;
    }
    let groups = build_lane_groups(
        git,
        lanes,
        &main_oid,
        args.order,
        &observer,
        &mut meta_cache,
    )?;
    let main_meta = main_metadata(args, git, &main_oid, &mut meta_cache)?;
    Ok(Analysis {
        groups,
        repository,
        main_meta,
    })
}

fn render_unfitted_analysis(
    args: &RuntimeOptions,
    analysis: &Analysis,
    environment: RenderEnvironment,
) -> RenderPlan {
    let colours = Colours::new(environment.colour_enabled(args.colour_mode));
    let session = RenderSession::new(
        args,
        &analysis.repository,
        analysis.main_meta.as_ref(),
        environment,
        &colours,
    );
    if analysis.groups.is_empty() {
        render_empty_selection(&session)
    } else {
        render_populated_selection(&analysis.groups, &session)
    }
}

fn render_analysis(
    args: &RuntimeOptions,
    analysis: &Analysis,
    environment: RenderEnvironment,
) -> RenderPlan {
    let mut plan = render_unfitted_analysis(args, analysis, environment);
    crate::render::fit_diff_stats(&mut plan.lines, environment.terminal_width());
    plan
}

fn hydrate_diff_stats(
    args: &RuntimeOptions,
    analysis: &mut Analysis,
    git: &dyn GitBackend,
    environment: RenderEnvironment,
) -> Result<()> {
    if !args.diff_stat {
        return Ok(());
    }
    if let Some(width) = environment.terminal_width() {
        set_placeholder_stats(
            &mut analysis.groups,
            Some(crate::model::DiffStat::default()),
        );
        let candidate = render_unfitted_analysis(args, analysis, environment);
        let fits = candidate
            .lines
            .iter()
            .filter_map(RenderLine::diff_stat_minimum_width)
            .all(|minimum| minimum <= width);
        set_placeholder_stats(&mut analysis.groups, None);
        if !fits {
            crate::diagnostics::event(format_args!("diff_stats=skipped reason=terminal_width"));
            return Ok(());
        }
    }
    populate_diff_stats(&mut analysis.groups, git, true)
}

fn set_placeholder_stats(groups: &mut [LaneGroup], value: Option<crate::model::DiffStat>) {
    for point in groups
        .iter_mut()
        .flat_map(|group| &mut group.lanes)
        .flat_map(|lane| &mut lane.branch_points)
    {
        point.diff_stat = value;
    }
}

fn build_render_plan(
    args: &RuntimeOptions,
    git: &dyn GitBackend,
    environment: RenderEnvironment,
) -> Result<RenderPlan> {
    let git = crate::backend::Session::new(git);
    let mut analysis = build_analysis(args, &git)?;
    hydrate_diff_stats(args, &mut analysis, &git, environment)?;
    Ok(render_analysis(args, &analysis, environment))
}

fn write_render_plan<W: Write>(
    stdout: &mut W,
    plan: &RenderPlan,
    environment: RenderEnvironment,
) -> Result<()> {
    let mut perf = crate::diagnostics::stage("write_output");
    perf.detail(format_args!("status=error"));
    for line in plan.lines() {
        write_rendered_line(stdout, line, environment)?;
    }
    stdout.flush()?;
    perf.detail(format_args!(
        "status=ok rendered_lines={}",
        plan.lines.len()
    ));
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
    write_render_plan(stdout, &plan, environment)?;
    Ok(())
}

#[cfg(test)]
mod tests;

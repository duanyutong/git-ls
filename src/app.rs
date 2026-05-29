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
    Colours, RenderContext, calculate_metadata_widths, render_lane_groups, render_main_tip,
    render_omitted_main_past, render_top_spacer,
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
        RenderContext {
            main_name: &self.repository.main_name,
            main_meta: self.main_meta,
            current_branch: self.repository.current_branch.as_deref(),
            head: self.repository.head.as_deref(),
            now_timestamp: self.now_timestamp,
            verbosity: self.verbosity,
            metadata_widths: calculate_metadata_widths(
                groups,
                self.main_meta,
                self.now_timestamp,
                self.verbosity,
            ),
            colours: self.colours,
        }
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
    RenderPlan::new(vec![
        render_top_spacer(session.colours, false),
        render_main_tip(&context),
        render_omitted_main_past(session.colours),
    ])
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
pub fn run_from_env() -> Result<()> {
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
mod tests {
    use clap::error::ErrorKind;

    use super::*;
    use crate::cli::{Args, Backend, ColourMode, Order, Palette, Verbosity};
    use crate::error::GitLsError;
    use crate::model::{BranchAnnotation, BranchPoint, Lane};
    use crate::test_support::{MockGit, TEST_NOW};

    fn clap_error_kind(error: GitLsError) -> ErrorKind {
        match error {
            GitLsError::Cli(error) => error.kind(),
            error => panic!("expected clap error, got {error:?}"),
        }
    }

    fn runtime_options(revset: &str, verbosity: Verbosity) -> RuntimeOptions {
        RuntimeOptions {
            revset: revset.to_string(),
            hidden: false,
            verbosity,
            backend: Backend::Shell,
            order: Order::Newest,
            colour_mode: ColourMode::Never,
            palette: Palette::Classic,
        }
    }

    fn repository_snapshot(current_branch: Option<&str>, head: Option<&str>) -> RepositorySnapshot {
        RepositorySnapshot {
            current_branch: current_branch.map(str::to_string),
            head: head.map(str::to_string),
            main_name: "main".to_string(),
        }
    }

    fn commit_meta(oid: &str, timestamp: i64, subject: &str) -> CommitMeta {
        CommitMeta {
            oid: oid.to_string(),
            short_oid: oid.chars().take(7).collect(),
            subject: subject.to_string(),
            timestamp,
        }
    }

    #[test]
    fn parses_default_arguments() {
        assert_eq!(
            parse_args_from(Vec::<String>::new()).unwrap(),
            Args {
                revset: "draft()".to_string(),
                hidden: false,
                verbose: 0,
                backend: None,
                order: None,
                colour_mode: None,
                palette: None,
            }
        );
    }

    #[test]
    fn parses_flags_and_revset() {
        assert_eq!(
            parse_args_from([
                "--hidden",
                "--backend=shell",
                "--order=oldest",
                "--colour",
                "never",
                "--palette",
                "tableau",
                "draft() & branches(feature/)",
            ])
            .unwrap(),
            Args {
                revset: "draft() & branches(feature/)".to_string(),
                hidden: true,
                verbose: 0,
                backend: Some(Backend::Shell),
                order: Some(Order::Oldest),
                colour_mode: Some(ColourMode::Never),
                palette: Some(Palette::Tableau),
            }
        );
    }

    #[test]
    fn parses_palette_names() {
        assert_eq!(
            parse_args_from(["-p", "classic"]).unwrap().palette,
            Some(Palette::Classic)
        );
        assert_eq!(
            parse_args_from(["--palette", "okabe"]).unwrap().palette,
            Some(Palette::Okabe)
        );
    }

    #[test]
    fn parses_additional_palette_names() {
        assert_eq!(
            parse_args_from(["-p", "set1"]).unwrap().palette,
            Some(Palette::Set1)
        );
        assert_eq!(
            parse_args_from(["-p", "paired"]).unwrap().palette,
            Some(Palette::Paired)
        );
        assert_eq!(
            parse_args_from(["-p", "bold"]).unwrap().palette,
            Some(Palette::Bold)
        );
        assert_eq!(
            parse_args_from(["-p", "vivid"]).unwrap().palette,
            Some(Palette::Vivid)
        );
        assert_eq!(
            parse_args_from(["-p", "tol"]).unwrap().palette,
            Some(Palette::Tol)
        );
    }

    #[test]
    fn parses_dash_prefixed_revset_after_separator() {
        assert_eq!(
            parse_args_from(["--", "-synthetic-revset"]).unwrap(),
            Args {
                revset: "-synthetic-revset".to_string(),
                hidden: false,
                verbose: 0,
                backend: None,
                order: None,
                colour_mode: None,
                palette: None,
            }
        );
    }

    #[test]
    fn parses_verbose_flag() {
        assert_eq!(parse_args_from(["-v"]).unwrap().verbose, 1);
        assert_eq!(parse_args_from(["--verbose"]).unwrap().verbose, 1);
        assert_eq!(parse_args_from(["-vv"]).unwrap().verbose, 2);
    }

    #[test]
    fn parses_help_without_requiring_git() {
        assert_eq!(
            clap_error_kind(parse_args_from(["--help"]).unwrap_err()),
            ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn rejects_invalid_arguments() {
        assert_eq!(
            clap_error_kind(parse_args_from(["--order"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--order=later"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--colour=maybe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--palette=maybe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--palette=safe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--palette=okabe-ito"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--unknown"]).unwrap_err()),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["one", "two"]).unwrap_err()),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn builds_render_plan_for_empty_selection_without_output_writer() {
        let revset = "((draft()) & branches()) - public()";
        let timestamp = TEST_NOW - 120;
        let git = MockGit::default()
            .with(&["branchless", "query", "-r", "main()"], "main-oid")
            .with(&["branchless", "query", "-b", revset], "")
            .with(
                &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
                "main-oid\nmain",
            )
            .with(&["config", "--get", "branchless.core.mainBranch"], "")
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "main-oid",
                ],
                &format!("main-oid\x00{timestamp}\x00main tip\x1e"),
            );
        let args = runtime_options("draft()", Verbosity::Medium);
        let environment = RenderEnvironment::new(TEST_NOW, None, false);

        let plan = build_render_plan(&args, &git, environment).unwrap();

        assert_eq!(
            plan,
            RenderPlan::new(vec![
                "  ⁝".to_string(),
                "▶ ◆── 2m (-, main-oi) main".to_string(),
                "  ⁝".to_string(),
            ])
        );
        assert!(
            git.calls()
                .iter()
                .all(|call| call.first().is_none_or(|arg| arg != "for-each-ref"))
        );
    }

    #[test]
    fn write_render_plan_is_the_output_boundary() {
        let plan = RenderPlan::new(vec!["abcdefgh".to_string(), "xy".to_string()]);
        let environment = RenderEnvironment::new(TEST_NOW, Some(5), false);
        let mut output = Vec::new();

        write_render_plan(&mut output, &plan, environment).unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "ab...\nxy\n");
    }

    #[test]
    fn render_session_supplies_shared_context_to_populated_plan() {
        let args = runtime_options("draft()", Verbosity::Medium);
        let colours = Colours::new(false, Palette::Classic);
        let repository = repository_snapshot(Some("feature"), Some("feature-oid"));
        let main_meta = commit_meta("main-oid", TEST_NOW - 120, "main tip");
        let branch_meta = commit_meta("feature-oid", TEST_NOW - 60, "feature tip");
        let groups = vec![LaneGroup {
            base_oid: Some("main-oid".to_string()),
            base_meta: None,
            main_distance: Some(0),
            lanes: vec![Lane {
                head_oid: "feature-oid".to_string(),
                base_oid: Some("main-oid".to_string()),
                branch_points: vec![BranchPoint {
                    oid: "feature-oid".to_string(),
                    names: vec!["feature".to_string()],
                    annotation: Some(BranchAnnotation {
                        meta: branch_meta,
                        commit_count: 1,
                    }),
                }],
                head_timestamp: TEST_NOW - 60,
                contains_current: true,
            }],
        }];
        let session = RenderSession::new(
            &args,
            &repository,
            Some(&main_meta),
            RenderEnvironment::new(TEST_NOW, None, false),
            &colours,
        );

        let plan = render_populated_selection(&groups, &session);

        assert!(
            plan.lines
                .iter()
                .any(|line| line.contains("1m (1, feature) feature"))
        );
        assert!(
            plan.lines
                .iter()
                .any(|line| line.contains("2m (-, main-oi) main"))
        );
    }

    #[test]
    fn run_renders_empty_selection_as_trunk() {
        let revset = "((draft()) & branches()) - public()";
        let timestamp = TEST_NOW - 120;
        let git = MockGit::default()
            .with(&["branchless", "query", "-r", "main()"], "main-oid")
            .with(&["branchless", "query", "-b", revset], "")
            .with(
                &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
                "main-oid\nmain",
            )
            .with(&["config", "--get", "branchless.core.mainBranch"], "")
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "main-oid",
                ],
                &format!("main-oid\x00{timestamp}\x00main tip\x1e"),
            );
        let mut output = Vec::new();

        run(["--color", "never"], &git, &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "  ⁝\n▶ ◆── 2m (-, main-oi) main\n  ⁝\n"
        );
        assert!(
            git.calls()
                .iter()
                .all(|call| call.first().is_none_or(|arg| arg != "for-each-ref"))
        );
        assert_eq!(
            git.calls(),
            vec![
                vec![
                    "config".to_string(),
                    "--get".to_string(),
                    "git-ls.verbosity".to_string()
                ],
                vec![
                    "config".to_string(),
                    "--get".to_string(),
                    "git-ls.backend".to_string()
                ],
                vec![
                    "config".to_string(),
                    "--get".to_string(),
                    "git-ls.palette".to_string()
                ],
                vec![
                    "branchless".to_string(),
                    "query".to_string(),
                    "-r".to_string(),
                    "main()".to_string()
                ],
                vec![
                    "branchless".to_string(),
                    "query".to_string(),
                    "-b".to_string(),
                    revset.to_string()
                ],
                vec![
                    "rev-parse".to_string(),
                    "HEAD".to_string(),
                    "--abbrev-ref".to_string(),
                    "HEAD".to_string()
                ],
                vec![
                    "config".to_string(),
                    "--get".to_string(),
                    "branchless.core.mainBranch".to_string()
                ],
                vec![
                    "show".to_string(),
                    "-s".to_string(),
                    "--format=%H%x00%ct%x00%s%x1e".to_string(),
                    "--no-walk=unsorted".to_string(),
                    "main-oid".to_string()
                ]
            ]
        );
    }
}

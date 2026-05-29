use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};

use clap::Parser;

#[cfg(test)]
use crate::backend::GitCommand;
use crate::backend::{GitBackend, GixBackend, ProcessGit, get_commit_meta};
use crate::cli::{Args, Backend, RuntimeOptions, read_git_ls_config};
use crate::error::Result;
use crate::lanes::{build_lane_groups, build_lanes, ordered_lanes};
use crate::model::BuiltLanes;
use crate::render::{
    Colours, RenderContext, calculate_metadata_widths, current_unix_timestamp, render_lane_groups,
    render_main_tip, render_omitted_main_past, render_top_spacer,
};
use crate::terminal::{terminal_output_width, write_rendered_line};

fn parse_args_from<I, S>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let cli_args =
        std::iter::once(OsString::from("git ls")).chain(args.into_iter().map(Into::into));
    Args::try_parse_from(cli_args).map_err(Into::into)
}

fn execute<W, G>(
    args: &RuntimeOptions,
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
    let groups = build_lane_groups(git, lanes, &main_oid, args.order, &mut meta_cache)?;
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

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;
    use crate::cli::{Args, Backend, ColourMode, Order, Palette};
    use crate::error::GitLsError;
    use crate::render::current_unix_timestamp;
    use crate::test_support::MockGit;

    fn clap_error_kind(error: GitLsError) -> ErrorKind {
        match error {
            GitLsError::Cli(error) => error.kind(),
            error => panic!("expected clap error, got {error:?}"),
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
    fn run_renders_empty_selection_as_trunk() {
        let revset = "((draft()) & branches()) - public()";
        let timestamp = current_unix_timestamp() - 120;
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

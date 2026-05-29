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

fn populated_workflow_git(config_verbosity: &str) -> MockGit {
    let revset = "((draft()) & branches()) - public()";
    let heads_revset = "heads(((draft()) & branches()) - public())";
    let feature_timestamp = TEST_NOW - 60;
    let main_timestamp = TEST_NOW - 120;

    MockGit::default()
        .with(&["config", "--get", "git-ls.verbosity"], config_verbosity)
        .with(&["config", "--get", "git-ls.backend"], "")
        .with(&["config", "--get", "git-ls.palette"], "")
        .with(&["branchless", "query", "-r", "main()"], "main-oid")
        .with(&["branchless", "query", "-b", revset], "feature")
        .with(
            &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
            "feature-oid\nfeature",
        )
        .with(&["config", "--get", "branchless.core.mainBranch"], "")
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "feature-oid\x00feature",
        )
        .with(&["branchless", "query", "-r", heads_revset], "feature-oid")
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "feature-oid",
            ],
            &format!("feature-oid\x00{feature_timestamp}\x00feature tip\x1e"),
        )
        .with(&["merge-base", "main-oid", "feature-oid"], "main-oid")
        .with(
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                "main-oid..feature-oid",
            ],
            "feature-oid",
        )
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "main-oid",
            ],
            &format!("main-oid\x00{main_timestamp}\x00main tip\x1e"),
        )
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
fn run_uses_git_config_verbosity_in_mocked_workflow() {
    let git = populated_workflow_git("2");
    let mut output = Vec::new();

    run(["--color", "never"], &git, &mut output).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("feature tip"));
}

#[test]
fn run_prefers_cli_verbosity_over_git_config() {
    let git = populated_workflow_git("2");
    let mut output = Vec::new();

    run(["--color", "never", "-v"], &git, &mut output).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("1m (1, feature) feature"));
    assert!(!output.contains("feature tip"));
}

#[test]
fn run_passes_hidden_selection_to_branchless_queries() {
    let revset = "((draft()) & branches()) - public()";
    let git = MockGit::default()
        .with(&["config", "--get", "git-ls.verbosity"], "")
        .with(&["config", "--get", "git-ls.backend"], "")
        .with(&["config", "--get", "git-ls.palette"], "")
        .with(
            &["branchless", "query", "-r", "--hidden", "main()"],
            "main-oid",
        )
        .with(&["branchless", "query", "-b", "--hidden", revset], "")
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
            &format!("main-oid\x00{}\x00main tip\x1e", TEST_NOW - 120),
        );
    let mut output = Vec::new();

    run(["--color", "never", "--hidden"], &git, &mut output).unwrap();

    assert!(
        git.calls()
            .iter()
            .any(|call| call == &["branchless", "query", "-r", "--hidden", "main()"])
    );
    assert!(
        git.calls()
            .iter()
            .any(|call| call == &["branchless", "query", "-b", "--hidden", revset])
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

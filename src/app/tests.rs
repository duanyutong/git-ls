use clap::error::ErrorKind;
use std::collections::HashMap;

use super::*;
use crate::cli::{Args, Backend, ColourMode, Layout, Order, Palette, Verbosity};
use crate::error::GitLsError;
use crate::model::{BranchAnnotation, BranchPoint, Lane};
use crate::test_support::{MockGit, TEST_NOW};

fn missing_mock_response(args: &[&str]) -> String {
    format!("missing mock git response: {}", args.join(" "))
}

fn missing_commit_meta_response(oid: &str) -> String {
    missing_mock_response(&[
        "show",
        "-s",
        "--format=%H%x00%ct%x00%s%x1e",
        "--no-walk=unsorted",
        oid,
    ])
}

fn owned_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

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
        debug: false,
        verbosity,
        backend: Backend::Shell,
        order: Order::Newest,
        colour_mode: ColourMode::Never,
        palette: Palette::Classic,
        layout: Layout::Inline,
    }
}

fn repository_snapshot(current_branch: Option<&str>, head: Option<&str>) -> RepositorySnapshot {
    RepositorySnapshot::new(
        current_branch.map(str::to_string),
        head.map(str::to_string),
        "main",
    )
}

fn commit_meta(oid: &str, timestamp: i64, subject: &str) -> CommitMeta {
    CommitMeta::new(oid, timestamp, subject)
}

#[derive(Default)]
struct RecordingWriter {
    bytes: Vec<u8>,
    fail_writes: bool,
}

impl RecordingWriter {
    fn failing() -> Self {
        Self {
            bytes: Vec::new(),
            fail_writes: true,
        }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn into_string(self) -> String {
        String::from_utf8(self.bytes).unwrap()
    }
}

impl std::io::Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.fail_writes {
            return Err(std::io::Error::other("closed"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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

fn empty_selection_git(timestamp: i64) -> MockGit {
    let revset = "((draft()) & branches()) - public()";
    MockGit::default()
        .with(&["branchless", "query", "-r", "main()"], "main-oid")
        .with(&["branchless", "query", "-b", revset], "")
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main",
        )
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
        )
}

#[test]
fn parses_default_arguments() {
    assert_eq!(
        parse_args_from(Vec::<String>::new()).unwrap(),
        Args {
            revset: "draft()".to_string(),
            hidden: false,
            debug: false,
            verbose: 0,
            verbosity: None,
            backend: None,
            order: None,
            colour_mode: None,
            palette: None,
            layout: None,
        }
    );
}

#[test]
fn parses_flags_and_revset() {
    assert_eq!(
        parse_args_from([
            "--hidden",
            "--debug",
            "--backend=shell",
            "--order=oldest",
            "--colour",
            "never",
            "--palette",
            "tableau",
            "--layout",
            "inline",
            "draft() & branches(feature/)",
        ])
        .unwrap(),
        Args {
            revset: "draft() & branches(feature/)".to_string(),
            hidden: true,
            debug: true,
            verbose: 0,
            verbosity: None,
            backend: Some(Backend::Shell),
            order: Some(Order::Oldest),
            colour_mode: Some(ColourMode::Never),
            palette: Some(Palette::Tableau),
            layout: Some(Layout::Inline),
        }
    );
}

#[test]
fn parses_layout_names() {
    assert_eq!(
        parse_args_from(["--layout", "columns"]).unwrap().layout,
        Some(Layout::Columns)
    );
    assert_eq!(
        parse_args_from(["--layout", "inline"]).unwrap().layout,
        Some(Layout::Inline)
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
            debug: false,
            verbose: 0,
            verbosity: None,
            backend: None,
            order: None,
            colour_mode: None,
            palette: None,
            layout: None,
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
fn parses_numeric_verbosity_option() {
    assert_eq!(
        parse_args_from(["--verbosity", "0"]).unwrap().verbosity,
        Some(0)
    );
    assert_eq!(
        parse_args_from(["--verbosity", "1"]).unwrap().verbosity,
        Some(1)
    );
    assert_eq!(
        parse_args_from(["--verbosity", "2"]).unwrap().verbosity,
        Some(2)
    );
    assert_eq!(
        clap_error_kind(parse_args_from(["--verbosity", "3"]).unwrap_err()),
        ErrorKind::ValueValidation
    );
}

#[test]
fn parses_help_without_requiring_git() {
    assert_eq!(
        clap_error_kind(parse_args_from(["--help"]).unwrap_err()),
        ErrorKind::DisplayHelp
    );
}

#[test]
fn parses_version_without_requiring_git() {
    assert_eq!(
        clap_error_kind(parse_args_from(["--version"]).unwrap_err()),
        ErrorKind::DisplayVersion
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
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main",
        )
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
            "▶ ◆── 2m (-) main".to_string(),
            "  ⁝".to_string(),
        ])
    );
}

#[test]
fn build_render_plan_falls_back_to_plain_git_when_branchless_is_unavailable() {
    let git = MockGit::default()
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main\nbase\x00feature/base\ntip\x00feature/tip",
        )
        .with(
            &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
            "tip\nfeature/tip",
        )
        .with(&["merge-base", "base", "main-oid"], "main-oid")
        .with(&["merge-base", "tip", "main-oid"], "main-oid")
        .with(&["merge-base", "base", "tip"], "base")
        .with(&["merge-base", "tip", "base"], "base")
        .with(&["merge-base", "main-oid", "tip"], "main-oid")
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "tip",
            ],
            &format!("tip\x00{}\x00tip\x1e", TEST_NOW - 60),
        )
        .with(
            &["rev-list", "--reverse", "--ancestry-path", "main-oid..tip"],
            "base\ntip",
        );
    let mut args = runtime_options("draft()", Verbosity::Low);
    args.debug = true;
    let environment = RenderEnvironment::new(TEST_NOW, None, false);

    let plan = build_render_plan(&args, &git, environment).unwrap();

    assert!(plan.lines.iter().any(|line| line.contains("feature/base")));
    assert!(plan.lines.iter().any(|line| line.contains("feature/tip")));
    assert!(
        git.calls()
            .iter()
            .any(|call| call == &["branchless", "query", "-r", "main()"])
    );
}

#[test]
fn skips_main_metadata_lookup_when_metadata_is_not_rendered() {
    let args = runtime_options("draft()", Verbosity::Low);
    let git = MockGit::default();
    let mut cache = HashMap::new();

    let meta = main_metadata(&args, &git, "main-oid", &mut cache).unwrap();

    assert_eq!(meta, None);
    assert!(git.calls().is_empty());
}

#[test]
fn propagates_main_metadata_lookup_errors() {
    let args = runtime_options("draft()", Verbosity::Medium);
    let git = MockGit::default();
    let mut cache = HashMap::new();

    let error = main_metadata(&args, &git, "main-oid", &mut cache).unwrap_err();

    assert_eq!(error.to_string(), missing_commit_meta_response("main-oid"));
}

#[test]
fn write_render_plan_is_the_output_boundary() {
    let plan = RenderPlan::new(vec!["abcdefgh".to_string(), "xy".to_string()]);
    let environment = RenderEnvironment::new(TEST_NOW, Some(5), false);
    let mut output = RecordingWriter::default();

    write_render_plan(&mut output, &plan, environment).unwrap();

    assert_eq!(output.into_string(), "ab...\nxy\n");
}

#[test]
fn write_render_plan_writes_empty_plan_as_noop() {
    let plan = RenderPlan::new(Vec::<String>::new());
    let environment = RenderEnvironment::new(TEST_NOW, None, false);
    let mut output = RecordingWriter::default();

    write_render_plan(&mut output, &plan, environment).unwrap();

    assert!(output.is_empty());
}

#[test]
fn write_render_plan_propagates_output_errors() {
    let plan = RenderPlan::new(vec!["line".to_string()]);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);
    let mut writer = RecordingWriter::failing();

    let error = write_render_plan(&mut writer, &plan, environment).unwrap_err();

    assert_eq!(error.to_string(), "failed to write output: closed");
}

#[test]
fn render_session_supplies_shared_context_to_populated_plan() {
    let args = runtime_options("draft()", Verbosity::Medium);
    let colours = Colours::new(false, Palette::Classic);
    let repository = repository_snapshot(Some("feature"), Some("feature-oid"));
    let main_meta = commit_meta("main-oid", TEST_NOW - 120, "main tip");
    let branch_meta = commit_meta("feature-oid", TEST_NOW - 60, "feature tip");
    let groups = vec![LaneGroup::new(
        Some("main-oid".to_string()),
        None,
        Some(0),
        vec![Lane::new(
            "feature-oid",
            Some("main-oid".to_string()),
            vec![BranchPoint::new(
                "feature-oid",
                ["feature"],
                Some(BranchAnnotation::new(branch_meta, 1)),
            )],
            TEST_NOW - 60,
            true,
        )],
    )];
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
            .any(|line| line.contains("1m (1) feature"))
    );
    assert!(plan.lines.iter().any(|line| line.contains("2m (-) main")));
}

#[test]
fn execute_writes_successful_render_plan() {
    let args = runtime_options("draft()", Verbosity::Medium);
    let git = empty_selection_git(TEST_NOW - 120);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);
    let mut output = RecordingWriter::default();

    execute(&args, &git, &mut output, environment).unwrap();

    assert_eq!(output.into_string(), "  ⁝\n▶ ◆── 2m (-) main\n  ⁝\n");
}

#[test]
fn debug_mode_does_not_change_render_plan() {
    let args = runtime_options("draft()", Verbosity::Medium);
    let mut debug_args = args.clone();
    debug_args.debug = true;
    let environment = RenderEnvironment::new(TEST_NOW, None, false);

    let plan = build_render_plan(&args, &empty_selection_git(TEST_NOW - 120), environment).unwrap();
    let debug_plan = build_render_plan(
        &debug_args,
        &empty_selection_git(TEST_NOW - 120),
        environment,
    )
    .unwrap();

    assert_eq!(debug_plan, plan);
}

#[test]
fn run_uses_git_config_verbosity_in_mocked_workflow() {
    let git = populated_workflow_git("2");
    let mut output = RecordingWriter::default();

    run(owned_args(&["--color", "never"]), &git, &mut output).unwrap();

    let output = output.into_string();
    assert!(output.contains("feature tip"));
}

#[test]
fn run_prefers_cli_verbosity_over_git_config() {
    let git = populated_workflow_git("2");
    let mut output = RecordingWriter::default();

    run(owned_args(&["--color", "never", "-v"]), &git, &mut output).unwrap();

    let output = output.into_string();
    assert!(output.contains("1m (1) feature"));
    assert!(!output.contains("feature tip"));
}

#[test]
fn build_render_plan_propagates_lane_selection_errors() {
    let args = runtime_options("draft()", Verbosity::Low);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);
    let git = MockGit::default();

    let error = build_render_plan(&args, &git, environment).unwrap_err();

    assert_eq!(
        error.to_string(),
        missing_mock_response(&[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:short)",
            "refs/heads"
        ])
    );
}

#[test]
fn execute_propagates_plan_errors_before_writing() {
    let args = runtime_options("draft()", Verbosity::Low);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);
    let git = MockGit::default();
    let mut output = RecordingWriter::default();

    let error = execute(&args, &git, &mut output, environment).unwrap_err();

    assert_eq!(
        error.to_string(),
        missing_mock_response(&[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:short)",
            "refs/heads"
        ])
    );
    assert!(output.is_empty());
}

#[test]
fn build_render_plan_propagates_empty_selection_main_metadata_errors() {
    let revset = "((draft()) & branches()) - public()";
    let git = MockGit::default()
        .with(&["branchless", "query", "-r", "main()"], "main-oid")
        .with(&["branchless", "query", "-b", revset], "");
    let git = git
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main",
        )
        .with(&["config", "--get", "branchless.core.mainBranch"], "")
        .with(
            &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
            "main-oid\nmain",
        );
    let args = runtime_options("draft()", Verbosity::Medium);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);

    let error = build_render_plan(&args, &git, environment).unwrap_err();

    assert_eq!(error.to_string(), missing_commit_meta_response("main-oid"));
}

#[test]
fn build_render_plan_propagates_populated_group_metadata_errors() {
    let revset = "((draft()) & branches()) - public()";
    let heads_revset = "heads(((draft()) & branches()) - public())";
    let git = MockGit::default()
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
            &format!("feature-oid\x00{}\x00feature tip\x1e", TEST_NOW - 60),
        )
        .with(&["merge-base", "main-oid", "feature-oid"], "old-base")
        .with(
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                "old-base..feature-oid",
            ],
            "feature-oid",
        );
    let args = runtime_options("draft()", Verbosity::Low);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);

    let error = build_render_plan(&args, &git, environment).unwrap_err();

    assert_eq!(error.to_string(), missing_commit_meta_response("old-base"));
}

#[test]
fn build_render_plan_propagates_populated_main_metadata_errors() {
    let revset = "((draft()) & branches()) - public()";
    let heads_revset = "heads(((draft()) & branches()) - public())";
    let git = MockGit::default()
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
            &format!("feature-oid\x00{}\x00feature tip\x1e", TEST_NOW - 60),
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
        );
    let args = runtime_options("draft()", Verbosity::Medium);
    let environment = RenderEnvironment::new(TEST_NOW, None, false);

    let error = build_render_plan(&args, &git, environment).unwrap_err();

    assert_eq!(error.to_string(), missing_commit_meta_response("main-oid"));
}

#[test]
fn run_propagates_argument_parse_errors_before_reading_config() {
    let git = MockGit::default();
    let mut output = RecordingWriter::default();

    let error = run(owned_args(&["--unknown"]), &git, &mut output).unwrap_err();

    assert_eq!(clap_error_kind(error), ErrorKind::UnknownArgument);
    assert!(git.calls().is_empty());
}

#[test]
fn run_propagates_git_config_errors_before_execution() {
    let git = MockGit::default().with(&["config", "--get", "git-ls.verbosity"], "full");
    let mut output = RecordingWriter::default();

    let error = run(owned_args(&["--color", "never"]), &git, &mut output).unwrap_err();

    assert_eq!(
        error.to_string(),
        "invalid git config git-ls.verbosity=\"full\": expected 0, 1, or 2"
    );
    assert!(output.is_empty());
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
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main",
        )
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
    let mut output = RecordingWriter::default();

    run(
        owned_args(&["--color", "never", "--hidden"]),
        &git,
        &mut output,
    )
    .unwrap();

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
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "main-oid\x00main",
        )
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
    let mut output = RecordingWriter::default();

    run(owned_args(&["--color", "never"]), &git, &mut output).unwrap();

    assert_eq!(output.into_string(), "  ⁝\n▶ ◆── 2m (-) main\n  ⁝\n");
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
                "config".to_string(),
                "--get".to_string(),
                "git-ls.layout".to_string()
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
                "for-each-ref".to_string(),
                "--format=%(objectname)%00%(refname:short)".to_string(),
                "refs/heads".to_string()
            ],
            vec![
                "config".to_string(),
                "--get".to_string(),
                "branchless.core.mainBranch".to_string()
            ],
            vec![
                "rev-parse".to_string(),
                "HEAD".to_string(),
                "--abbrev-ref".to_string(),
                "HEAD".to_string()
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

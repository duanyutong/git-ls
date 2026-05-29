use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command as TestCommand;

use clap::error::ErrorKind;
use tempfile::TempDir;

use crate::app::{parse_args_from, run};
use crate::backend::{
    GitBackend, GitCommand, GixBackend, lines, non_empty, shell_cache_commit_meta,
};
use crate::cli::{
    Args, Backend, ColourMode, DEFAULT_PALETTE, EffectiveArgs, GitLsConfig, Order, Palette,
    Verbosity, parse_verbosity_config, read_git_ls_config,
};
use crate::error::{GitLsError, Result};
use crate::lanes::{
    branch_points_by_oid, branch_revset, build_lane, build_lane_groups, build_lanes,
    grouped_by_base, ordered_lanes,
};
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, LaneGroup,
    display_short_oid,
};
use crate::render::{
    COLLAPSED_MAIN_GLYPH, Colours, LaneRenderLayout, MainSpine, MetadataWidths, RenderContext,
    TrunkLabel, calculate_metadata_widths, current_unix_timestamp, display_names, main_label,
    render_group, render_lane_groups, render_orphaned_group, trunk_prefix,
};
use crate::terminal::{TRUNCATION_TAIL, fit_line_to_terminal_width};

const TEST_NOW: i64 = 1_700_000_120;
const TEST_COMMIT_TIME: i64 = 1_700_000_000;

#[derive(Default)]
struct MockGit {
    responses: HashMap<Vec<String>, String>,
    calls: RefCell<Vec<Vec<String>>>,
}

impl MockGit {
    fn with(mut self, args: &[&str], output: &str) -> Self {
        self.responses.insert(
            args.iter().map(|arg| (*arg).to_string()).collect(),
            output.to_string(),
        );
        self
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}

impl GitCommand for MockGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        self.calls.borrow_mut().push(key.clone());
        if let Some(output) = self.responses.get(&key) {
            return Ok(output.clone());
        }
        if allow_failure {
            return Ok(String::new());
        }
        Err(GitLsError::TestFixture(format!(
            "missing mock git response: {}",
            args.join(" ")
        )))
    }
}

fn clap_error_kind(error: GitLsError) -> ErrorKind {
    match error {
        GitLsError::Cli(error) => error.kind(),
        error => panic!("expected clap error, got {error:?}"),
    }
}

fn point(oid: &str, names: &[&str]) -> BranchPoint {
    BranchPoint {
        oid: oid.to_string(),
        names: names.iter().map(|name| (*name).to_string()).collect(),
        annotation: None,
    }
}

fn point_with_count(oid: &str, names: &[&str], commit_count: usize, subject: &str) -> BranchPoint {
    point_with_count_at(oid, names, commit_count, subject, TEST_COMMIT_TIME)
}

fn point_with_count_at(
    oid: &str,
    names: &[&str],
    commit_count: usize,
    subject: &str,
    timestamp: i64,
) -> BranchPoint {
    BranchPoint {
        oid: oid.to_string(),
        names: names.iter().map(|name| (*name).to_string()).collect(),
        annotation: Some(BranchAnnotation {
            meta: CommitMeta {
                oid: oid.to_string(),
                short_oid: oid.to_string(),
                subject: subject.to_string(),
                timestamp,
            },
            commit_count,
        }),
    }
}

fn meta(oid: &str, subject: &str) -> CommitMeta {
    CommitMeta {
        oid: oid.to_string(),
        short_oid: oid.to_string(),
        subject: subject.to_string(),
        timestamp: 0,
    }
}

fn test_colours(enabled: bool) -> Colours {
    Colours {
        enabled,
        palette: DEFAULT_PALETTE.ansi_colours(),
    }
}

fn lane(oid: &str, base: Option<&str>, timestamp: i64, contains_current: bool) -> Lane {
    Lane {
        head_oid: oid.to_string(),
        base_oid: base.map(ToOwned::to_owned),
        branch_points: vec![point(oid, &[oid])],
        head_timestamp: timestamp,
        contains_current,
    }
}

#[test]
fn display_short_oid_uses_fixed_seven_character_width() {
    assert_eq!(
        display_short_oid("309567f69abcdef0123456789abcdef01234567"),
        "309567f"
    );
    assert_eq!(display_short_oid("abc123"), "abc123");
}

#[test]
fn shell_commit_metadata_uses_fixed_display_oid() {
    let mut cache = HashMap::new();

    shell_cache_commit_meta(
        "branch-head",
        "309567f69abcdef0123456789abcdef01234567\x001700000001\x00subject",
        &mut cache,
    )
    .unwrap();

    let meta = cache.get("branch-head").unwrap();
    assert_eq!(meta.oid, "309567f69abcdef0123456789abcdef01234567");
    assert_eq!(meta.short_oid, "309567f");
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = TestCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .to_string()
}

fn commit_file(repo: &Path, path: &str, content: &str, message: &str) -> String {
    std::fs::write(repo.join(path), content).unwrap();
    git(repo, &["add", path]);
    git(repo, &["commit", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn parity_repo() -> (TempDir, String, String, String) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "--initial-branch", "main"]);
    git(repo, &["config", "user.name", "git-ls tests"]);
    git(repo, &["config", "user.email", "git-ls@example.invalid"]);

    commit_file(repo, "root.txt", "root\n", "root");
    git(repo, &["checkout", "-b", "side"]);
    let side_oid = commit_file(repo, "side.txt", "side\n", "side before base");

    git(repo, &["checkout", "main"]);
    let base_oid = commit_file(repo, "base.txt", "base\n", "base");
    git(repo, &["checkout", "-b", "topic"]);
    commit_file(repo, "topic.txt", "topic\n", "topic");
    git(repo, &["merge", "--no-ff", "side", "-m", "merge side"]);
    let head_oid = git(repo, &["rev-parse", "HEAD"]);

    (temp, base_oid, head_oid, side_oid)
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
fn parses_integer_verbosity_config_only() {
    assert_eq!(
        parse_verbosity_config("git-ls.verbosity", "0").unwrap(),
        Verbosity::Low
    );
    assert_eq!(
        parse_verbosity_config("git-ls.verbosity", "1").unwrap(),
        Verbosity::Medium
    );
    assert_eq!(
        parse_verbosity_config("git-ls.verbosity", "2").unwrap(),
        Verbosity::High
    );

    assert!(matches!(
        parse_verbosity_config("git-ls.verbosity", "full"),
        Err(GitLsError::InvalidGitConfig { .. })
    ));
}

#[test]
fn reads_git_ls_config_defaults() {
    let git = MockGit::default()
        .with(&["config", "--get", "git-ls.verbosity"], "2")
        .with(&["config", "--get", "git-ls.backend"], "shell")
        .with(&["config", "--get", "git-ls.palette"], "okabe");

    let config = read_git_ls_config(&git).unwrap();
    let args = parse_args_from(Vec::<String>::new())
        .unwrap()
        .resolve(&config);

    assert_eq!(config.verbosity, Some(Verbosity::High));
    assert_eq!(config.backend, Some(Backend::Shell));
    assert_eq!(config.palette, Some(Palette::Okabe));
    assert_eq!(args.verbosity, Verbosity::High);
    assert_eq!(args.backend, Backend::Shell);
    assert_eq!(args.palette, Palette::Okabe);
}

#[test]
fn uses_medium_verbosity_by_default() {
    let args = parse_args_from(Vec::<String>::new())
        .unwrap()
        .resolve(&GitLsConfig::default());

    assert_eq!(args.verbosity, Verbosity::Medium);
}

#[test]
fn explicit_cli_options_override_git_ls_config() {
    let config = GitLsConfig {
        verbosity: Some(Verbosity::High),
        backend: Some(Backend::Shell),
        palette: Some(Palette::Okabe),
    };
    let args = parse_args_from(["-v", "--backend", "gix", "-p", "classic"])
        .unwrap()
        .resolve(&config);

    assert_eq!(args.verbosity, Verbosity::Medium);
    assert_eq!(args.backend, Backend::Gix);
    assert_eq!(args.palette, Palette::Classic);
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
fn normalises_line_output() {
    assert_eq!(
        lines("  a\n\n b \n\t\nc"),
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(non_empty("  value \n"), Some("value".to_string()));
    assert_eq!(non_empty(" \n"), None);
}

#[test]
fn creates_branch_revset() {
    assert_eq!(
        branch_revset("draft()"),
        "((draft()) & branches()) - public()"
    );
}

#[test]
fn maps_selected_branch_points_by_oid() {
    let mut branches = HashMap::new();
    branches.insert(
        "a".to_string(),
        vec!["zeta".to_string(), "alpha".to_string(), "main".to_string()],
    );
    branches.insert("b".to_string(), vec!["ignored".to_string()]);

    let selected = vec!["alpha".to_string(), "zeta".to_string()];
    let points = branch_points_by_oid(&selected, &branches);

    assert_eq!(points.len(), 1);
    assert_eq!(
        points.get("a").unwrap(),
        &BranchPointRef {
            oid: "a".to_string(),
            names: vec!["alpha".to_string(), "zeta".to_string()],
        }
    );
}

#[test]
fn gix_backend_matches_git_ancestry_path() {
    let (temp, base_oid, head_oid, side_oid) = parity_repo();
    let repo = temp.path();
    let backend = GixBackend::discover_from(repo).unwrap();
    let shell_path = lines(&git(
        repo,
        &[
            "rev-list",
            "--reverse",
            "--ancestry-path",
            &format!("{base_oid}..{head_oid}"),
        ],
    ));

    assert_eq!(
        backend.merge_base(&base_oid, &head_oid).unwrap(),
        Some(base_oid.clone())
    );
    assert_eq!(
        backend.ancestry_path(Some(&base_oid), &head_oid).unwrap(),
        shell_path
    );
    assert!(!shell_path.contains(&side_oid));
}

#[test]
fn gix_backend_reads_repository_snapshot_and_commit_metadata() {
    let (temp, _base_oid, head_oid, _side_oid) = parity_repo();
    let repo = temp.path();
    git(repo, &["config", "branchless.core.mainBranch", "trunk"]);

    let backend = GixBackend::discover_from(repo).unwrap();
    let (head, current_branch) = backend.current_head_and_branch().unwrap();
    let branches = backend.local_branches_by_oid().unwrap();
    let mut cache = HashMap::new();
    backend
        .cache_commit_metas(&[&head_oid], &mut cache)
        .unwrap();

    assert_eq!(head, Some(head_oid.clone()));
    assert_eq!(current_branch, Some("topic".to_string()));
    assert!(
        branches
            .get(&head_oid)
            .is_some_and(|names| names.contains(&"topic".to_string()))
    );
    assert_eq!(backend.main_branch_name().unwrap(), "trunk");
    let meta = cache.get(&head_oid).unwrap();
    assert_eq!(meta.short_oid, display_short_oid(&head_oid));
    assert_eq!(meta.subject, "merge side");
}

#[test]
fn orders_lanes_by_current_status_time_and_oid() {
    let lanes = vec![
        lane("older", Some("main"), 10, false),
        lane("current", Some("main"), 1, true),
        lane("newer-b", Some("main"), 20, false),
        lane("newer-a", Some("main"), 20, false),
    ];

    let newest: Vec<String> = ordered_lanes(lanes.clone(), Order::Newest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(newest, vec!["current", "newer-a", "newer-b", "older"]);

    let oldest: Vec<String> = ordered_lanes(lanes, Order::Oldest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(oldest, vec!["current", "older", "newer-a", "newer-b"]);
}

#[test]
fn groups_lanes_by_base_with_current_group_first() {
    let lanes = vec![
        lane("a", Some("base-a"), 1, false),
        lane("b", Some("base-b"), 2, true),
        lane("c", Some("base-a"), 3, false),
    ];

    let groups = grouped_by_base(lanes);

    assert_eq!(groups[0].0, Some("base-b".to_string()));
    assert_eq!(groups[0].1.len(), 1);
    assert_eq!(groups[1].0, Some("base-a".to_string()));
    assert_eq!(groups[1].1.len(), 2);
}

#[test]
fn builds_lane_groups_with_main_history_distances() {
    let git = MockGit::default()
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "old-main",
            ],
            "old-main\x001700000001\x00old base\x1e",
        )
        .with(
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                "old-main..main-oid",
            ],
            "main-1\nmain-2\nmain-oid",
        );
    let lanes = vec![
        lane("current", Some("main-oid"), 2, false),
        lane("old", Some("old-main"), 1, true),
    ];
    let mut cache = HashMap::new();

    let groups = build_lane_groups(&git, lanes, "main-oid", &mut cache).unwrap();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].base_oid, Some("main-oid".to_string()));
    assert_eq!(groups[0].main_distance, Some(0));
    assert_eq!(groups[0].base_meta, None);
    assert_eq!(groups[1].base_oid, Some("old-main".to_string()));
    assert_eq!(groups[1].main_distance, Some(3));
    assert_eq!(
        groups[1]
            .base_meta
            .as_ref()
            .map(|meta| meta.subject.as_str()),
        Some("old base")
    );
}

#[test]
fn counts_each_branch_point_from_previous_visible_stack_point() {
    let git = MockGit::default()
        .with(&["merge-base", "main-oid", "b"], "main-oid")
        .with(
            &["rev-list", "--reverse", "--ancestry-path", "main-oid..b"],
            "a\nb",
        );
    let mut points_by_oid = HashMap::new();
    points_by_oid.insert(
        "a".to_string(),
        BranchPointRef {
            oid: "a".to_string(),
            names: vec!["feature/one".to_string()],
        },
    );
    points_by_oid.insert(
        "b".to_string(),
        BranchPointRef {
            oid: "b".to_string(),
            names: vec!["feature/two".to_string()],
        },
    );
    let mut cache = HashMap::from([
        ("a".to_string(), meta("a", "first")),
        ("b".to_string(), meta("b", "second")),
    ]);

    let lane = build_lane(
        &git,
        "b",
        "main-oid",
        &points_by_oid,
        None,
        None,
        Verbosity::Medium,
        &mut cache,
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        lane.branch_points,
        vec![
            point_with_count_at("b", &["feature/two"], 1, "second", 0),
            point_with_count_at("a", &["feature/one"], 1, "first", 0),
        ]
    );
}

#[test]
fn renders_markers_names_and_trunk() {
    let colours = test_colours(false);
    let lanes = vec![
        Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        },
        Lane {
            head_oid: "b".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("b", &["feature/two"])],
            head_timestamp: 2,
            contains_current: true,
        },
    ];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature/two"),
        head: Some("b"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec![
            "    ◯   feature/one".to_string(),
            "▶ ⁝ │ ● feature/two".to_string(),
            "  ◇─┴─┘ main".to_string()
        ]
    );
}

#[test]
fn renders_exactly_one_future_line_above_main_node() {
    let colours = test_colours(false);
    let lanes = vec![
        Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        },
        Lane {
            head_oid: "b".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("b", &["feature/two"])],
            head_timestamp: 2,
            contains_current: true,
        },
    ];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature/two"),
        head: Some("b"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output
            .iter()
            .filter(|line| line.contains(COLLAPSED_MAIN_GLYPH))
            .count(),
        1
    );
    assert_eq!(output[output.len() - 2], "▶ ⁝ │ ● feature/two");
    assert_eq!(output[output.len() - 1], "  ◇─┴─┘ main");
}

#[test]
fn renders_branch_metadata_with_commit_count_for_multi_commit_branch() {
    let colours = test_colours(false);
    let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

    let label = display_names(
        &point,
        Some("other"),
        0,
        TEST_NOW,
        Verbosity::High,
        MetadataWidths::default(),
        &colours,
    );

    assert_eq!(label, "2m (3, branch-head) feature/topic finish topic");
}

#[test]
fn renders_summary_branch_metadata_without_commit_title() {
    let colours = test_colours(false);
    let point = point_with_count("branch-head", &["feature/topic"], 3, "finish topic");

    let label = display_names(
        &point,
        Some("other"),
        0,
        TEST_NOW,
        Verbosity::Medium,
        MetadataWidths::default(),
        &colours,
    );

    assert_eq!(label, "2m (3, branch-head) feature/topic");
}

#[test]
fn renders_single_main_based_lane_with_main_spine() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "a".to_string(),
        base_oid: Some("main".to_string()),
        branch_points: vec![point("a", &["feature/one"])],
        head_timestamp: 1,
        contains_current: true,
    }];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature/one"),
        head: Some("a"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec!["▶ ⁝ ● feature/one".to_string(), "  ◇─┘ main".to_string()]
    );
}

#[test]
fn renders_current_main_on_trunk_row() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "a".to_string(),
        base_oid: Some("main".to_string()),
        branch_points: vec![point("a", &["feature/one"])],
        head_timestamp: 1,
        contains_current: false,
    }];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("main"),
        head: Some("main"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_group(
        &lanes,
        lanes.len(),
        0,
        &ctx,
        TrunkLabel::Main,
        MainSpine::Future,
    );

    assert_eq!(
        output,
        vec!["  ⁝ ◯ feature/one".to_string(), "▶ ◆─┘ main".to_string()]
    );
}

#[test]
fn renders_orphaned_lane_with_single_warning_marker() {
    let colours = test_colours(false);
    let lanes = vec![Lane {
        head_oid: "backup".to_string(),
        base_oid: None,
        branch_points: vec![point("backup", &["test-branch-name"])],
        head_timestamp: 1,
        contains_current: false,
    }];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("main"),
        head: Some("main"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_orphaned_group(&lanes, &ctx);

    assert_eq!(
        output,
        vec!["  ⁝ ⦸ test-branch-name (orphaned)".to_string()]
    );
}

#[test]
fn renders_orphaned_only_groups_around_main_tip() {
    let colours = test_colours(false);
    let groups = vec![LaneGroup {
        base_oid: None,
        base_meta: None,
        main_distance: None,
        lanes: vec![Lane {
            head_oid: "backup".to_string(),
            base_oid: None,
            branch_points: vec![point("backup", &["test-branch-name"])],
            head_timestamp: 1,
            contains_current: false,
        }],
    }];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("main"),
        head: Some("main"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            "  ⁝".to_string(),
            "▶ ◆── main".to_string(),
            "  ⁝ ⦸ test-branch-name (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_main_metadata_in_aligned_annotation_column() {
    let colours = test_colours(false);
    let main_meta = CommitMeta {
        oid: "main-oid".to_string(),
        short_oid: "main-oi".to_string(),
        subject: "main tip".to_string(),
        timestamp: TEST_COMMIT_TIME,
    };
    let groups = vec![LaneGroup {
        base_oid: None,
        base_meta: None,
        main_distance: None,
        lanes: vec![Lane {
            head_oid: "backup-oid".to_string(),
            base_oid: None,
            branch_points: vec![point_with_count_at(
                "backup-oid",
                &["backup"],
                10,
                "backup tip",
                TEST_COMMIT_TIME,
            )],
            head_timestamp: TEST_COMMIT_TIME,
            contains_current: false,
        }],
    }];
    let metadata_widths =
        calculate_metadata_widths(&groups, Some(&main_meta), TEST_NOW, Verbosity::Medium);
    let ctx = RenderContext {
        main_name: "main",
        main_meta: Some(&main_meta),
        current_branch: Some("main"),
        head: Some("main-oid"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Medium,
        metadata_widths,
        colours: &colours,
    };

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            "  ⁝".to_string(),
            "▶ ◆── 2m (--, main-oi) main".to_string(),
            "  ⁝ ⦸ 2m (10, backup-oid) backup (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_orphaned_groups_below_connected_stacks() {
    let colours = test_colours(false);
    let groups = vec![
        LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: None,
            main_distance: Some(0),
            lanes: vec![Lane {
                head_oid: "feature".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("feature", &["feature/current"])],
                head_timestamp: 2,
                contains_current: true,
            }],
        },
        LaneGroup {
            base_oid: None,
            base_meta: None,
            main_distance: None,
            lanes: vec![
                Lane {
                    head_oid: "orphan-a".to_string(),
                    base_oid: None,
                    branch_points: vec![point("orphan-a", &["orphan-A"])],
                    head_timestamp: 1,
                    contains_current: false,
                },
                Lane {
                    head_oid: "orphan-b".to_string(),
                    base_oid: None,
                    branch_points: vec![point("orphan-b", &["orphan-B"])],
                    head_timestamp: 1,
                    contains_current: false,
                },
            ],
        },
    ];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature/current"),
        head: Some("feature"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            String::new(),
            "▶ ⁝ ● feature/current".to_string(),
            "  ◇─┘ main".to_string(),
            "  ⁝ ⦸ orphan-A (orphaned)".to_string(),
            "  ⁝ ⦸ orphan-B (orphaned)".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn renders_old_main_groups_with_collapsed_main_history() {
    let colours = test_colours(false);
    let groups = vec![
        LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: Some(meta("main", "main tip")),
            main_distance: Some(0),
            lanes: vec![
                Lane {
                    head_oid: "feature-one".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-one", &["feature/one"])],
                    head_timestamp: 4,
                    contains_current: false,
                },
                Lane {
                    head_oid: "feature-two".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-two", &["feature/two"])],
                    head_timestamp: 3,
                    contains_current: false,
                },
                Lane {
                    head_oid: "feature-current".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature-current", &["feature/current"])],
                    head_timestamp: 2,
                    contains_current: false,
                },
            ],
        },
        LaneGroup {
            base_oid: Some("old-main".to_string()),
            base_meta: Some(meta(
                "old-main",
                "chore: this is an old commit in main history",
            )),
            main_distance: Some(842),
            lanes: vec![Lane {
                head_oid: "old-feature".to_string(),
                base_oid: Some("old-main".to_string()),
                branch_points: vec![point("old-feature", &["dyt/tgs_api"])],
                head_timestamp: 1,
                contains_current: true,
            }],
        },
    ];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("dyt/tgs_api"),
        head: Some("old-feature"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_lane_groups(&groups, &ctx);

    assert_eq!(
        output,
        vec![
            String::new(),
            "    ◯     feature/one".to_string(),
            "    │ ◯   feature/two".to_string(),
            "  ⁝ │ │ ◯ feature/current".to_string(),
            "  ◇─┴─┴─┘ main".to_string(),
            "  │".to_string(),
            "  ⁝ (842 commits on main)".to_string(),
            "  │".to_string(),
            "▶ │     ● dyt/tgs_api".to_string(),
            "  ◇─────┘ chore: this is an old commit in main history".to_string(),
            "  ⁝".to_string(),
        ]
    );
}

#[test]
fn fits_plain_rows_to_terminal_width() {
    let line = "  ◯ feature/very-long-branch-name ci(package): publish generated api package";

    let fitted = fit_line_to_terminal_width(line, Some(24));

    assert_ne!(fitted.as_ref(), line);
    assert!(fitted.ends_with(TRUNCATION_TAIL));
    assert!(console::measure_text_width(fitted.as_ref()) <= 24);
}

#[test]
fn fits_coloured_rows_without_counting_ansi_sequences() {
    let colours = test_colours(true);
    let line = format!(
        "  {} {}",
        colours.stack(0, "feature/very-long-branch-name"),
        colours.commit_title("ci(package): publish generated api package")
    );

    let fitted = fit_line_to_terminal_width(&line, Some(24));
    let visible = console::strip_ansi_codes(fitted.as_ref());

    assert_ne!(fitted.as_ref(), line);
    assert!(fitted.contains("\x1b["));
    assert!(visible.ends_with(TRUNCATION_TAIL));
    assert!(console::measure_text_width(fitted.as_ref()) <= 24);
}

#[test]
fn fits_rows_to_tiny_terminal_widths() {
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(0)).as_ref(), "");
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(1)).as_ref(), ".");
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(2)).as_ref(), "..");
    assert_eq!(
        fit_line_to_terminal_width("abcdef", None).as_ref(),
        "abcdef"
    );
}

#[test]
fn colours_text_when_enabled() {
    let colours = test_colours(true);

    assert_eq!(colours.stack(0, "x"), "\x1b[38;5;41mx\x1b[0m");
    assert_eq!(
        colours.current_stack(0, "x"),
        "\x1b[1m\x1b[4m\x1b[38;5;41mx\x1b[0m"
    );
    assert_eq!(
        colours.current_indicator(0, "x"),
        "\x1b[1m\x1b[38;5;41mx\x1b[0m"
    );
    assert_eq!(colours.dim("x"), "\x1b[2mx\x1b[0m");
    assert_eq!(colours.muted_text("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_age("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_count("x"), "\x1b[38;5;255mx\x1b[0m");
    assert_eq!(colours.metadata_oid("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.metadata_punctuation("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.commit_title("x"), "\x1b[38;5;251mx\x1b[0m");
    assert_eq!(colours.orphaned_name("x"), "\x1b[38;5;255mx\x1b[0m");
    assert_eq!(colours.orphaned_glyph("x"), "\x1b[1m\x1b[38;5;255mx\x1b[0m");
    assert_eq!(
        colours.orphaned_status("x"),
        "\x1b[1m\x1b[38;5;255mx\x1b[0m"
    );
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("main"),
        head: Some("main"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };
    assert_eq!(main_label(&ctx), "\x1b[1m\x1b[4m\x1b[38;5;41mmain\x1b[0m");
    assert_eq!(
        trunk_prefix(LaneRenderLayout::empty(), true, MainSpine::Hidden, &colours),
        "\x1b[38;5;41m◆\x1b[0m\x1b[38;5;41m──\x1b[0m"
    );
    let inactive_main_ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature"),
        head: Some("feature"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };
    assert_eq!(main_label(&inactive_main_ctx), "\x1b[38;5;41mmain\x1b[0m");
    assert_eq!(
        trunk_prefix(
            LaneRenderLayout::empty(),
            false,
            MainSpine::Hidden,
            &colours
        ),
        "\x1b[38;5;41m◇\x1b[0m\x1b[38;5;41m──\x1b[0m"
    );
}

#[test]
fn main_reserves_first_palette_colour() {
    let colours = test_colours(true);
    let groups = vec![LaneGroup {
        base_oid: Some("main".to_string()),
        base_meta: None,
        main_distance: Some(0),
        lanes: vec![Lane {
            head_oid: "feature".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("feature", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        }],
    }];
    let ctx = RenderContext {
        main_name: "main",
        main_meta: None,
        current_branch: Some("feature/one"),
        head: Some("feature"),
        now_timestamp: TEST_NOW,
        verbosity: Verbosity::Low,
        metadata_widths: MetadataWidths::default(),
        colours: &colours,
    };

    let output = render_lane_groups(&groups, &ctx);

    assert!(output[1].contains("\x1b[38;5;203m●\x1b[0m"));
    assert!(output[2].contains("\x1b[38;5;41m◇\x1b[0m"));
    assert!(output[2].contains("\x1b[38;5;41mmain\x1b[0m"));
}

#[test]
fn builds_lanes_with_mocked_git_boundary() {
    let revset = "((draft()) & branches()) - public()";
    let git = MockGit::default()
        .with(&["branchless", "query", "-r", "main()"], "main-oid")
        .with(
            &["branchless", "query", "-b", revset],
            "feature/one\nfeature/two\nchore/other",
        )
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "a\0feature/one\nb\0feature/two\nc\0chore/other\nmain-oid\0main",
        )
        .with(
            &["branchless", "query", "-r", &format!("heads({revset})")],
            "b\nc",
        )
        .with(
            &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
            "b\nfeature/two",
        )
        .with(&["config", "--get", "branchless.core.mainBranch"], "")
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "b",
                "c",
            ],
            "b\x001700000002\x00second\x1e\nc\x001700000001\x00third\x1e",
        )
        .with(&["merge-base", "main-oid", "b"], "main-oid")
        .with(
            &["rev-list", "--reverse", "--ancestry-path", "main-oid..b"],
            "a\nb",
        )
        .with(&["merge-base", "main-oid", "c"], "main-oid")
        .with(
            &["rev-list", "--reverse", "--ancestry-path", "main-oid..c"],
            "c",
        );
    let args = EffectiveArgs {
        revset: "draft()".to_string(),
        hidden: false,
        verbosity: Verbosity::Low,
        backend: Backend::Gix,
        order: Order::Newest,
        colour_mode: ColourMode::Never,
        palette: DEFAULT_PALETTE,
    };
    let mut cache = HashMap::new();

    let BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
    } = build_lanes(&git, &args, &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(repository.main_name, "main");
    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].head_oid, "b");
    assert!(lanes[0].contains_current);
    assert_eq!(
        lanes[0].branch_points,
        vec![point("b", &["feature/two"]), point("a", &["feature/one"])]
    );
    assert_eq!(lanes[1].head_oid, "c");
    assert!(!lanes[1].contains_current);

    let calls = git.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.first().is_some_and(|arg| arg == "for-each-ref"))
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call
                .get(2)
                .is_some_and(|arg| arg == "--format=%H%x00%ct%x00%s%x1e"))
            .count(),
        1
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

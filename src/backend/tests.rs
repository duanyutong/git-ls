use std::collections::HashMap;
use std::path::Path;
use std::process::Command as TestCommand;

use tempfile::TempDir;

use super::metadata::{
    GIT_SHOW_COMMIT_META_ARG, insert_commit_meta, missing_commit_aliases, parse_shell_commit_meta,
};
use super::shell::{lines, shell_cache_commit_metas};
use super::*;
use crate::error::{GitLsError, Result};
use crate::model::{CommitMeta, display_short_oid};
use crate::test_support::MockGit;

struct MetadataBackendStub {
    fail: bool,
}

impl MetadataBackendStub {
    fn empty() -> Self {
        Self { fail: false }
    }

    fn failing() -> Self {
        Self { fail: true }
    }
}

impl CommitMetadataBackend for MetadataBackendStub {
    fn cache_commit_metas(
        &self,
        _oids: &[&str],
        _cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        if self.fail {
            Err(GitLsError::TestFixture(
                "forced metadata backend failure".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

struct RepositoryCommandStub {
    output: &'static str,
    fail: bool,
}

impl RepositoryCommandStub {
    fn output(output: &'static str) -> Self {
        Self {
            output,
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            output: "",
            fail: true,
        }
    }
}

impl GitCommand for RepositoryCommandStub {
    fn run(&self, args: &[&str], _allow_failure: bool) -> Result<String> {
        if self.fail {
            Err(GitLsError::TestFixture(format!(
                "forced git failure: {}",
                args.join(" ")
            )))
        } else {
            Ok(self.output.to_string())
        }
    }
}

#[test]
fn shell_metadata_hydration_indexes_alias_and_full_oid() {
    let full_oid = "309567f69abcdef0123456789abcdef01234567";
    let git = MockGit::default().with(
        &[
            "show",
            "-s",
            GIT_SHOW_COMMIT_META_ARG,
            "--no-walk=unsorted",
            "branch-head",
        ],
        &format!("{full_oid}\x001700000001\x00subject\x1e"),
    );
    let mut cache = HashMap::new();

    shell_cache_commit_metas(&git, &["branch-head"], &mut cache).unwrap();

    let meta = cache.get("branch-head").unwrap();
    assert_eq!(meta.oid, full_oid);
    assert_eq!(meta.short_oid, "309567f");
    assert_eq!(cache.get(full_oid), Some(meta));
}

#[test]
fn metadata_cache_deduplicates_missing_aliases() {
    let cached_meta = CommitMeta::new("cached", 1, "cached");
    let mut cache = HashMap::new();
    insert_commit_meta(&mut cache, "cached-alias", cached_meta);

    assert_eq!(
        missing_commit_aliases(&["cached", "cached-alias", "new", "new"], &cache),
        vec!["new"]
    );
}

#[test]
fn get_commit_meta_reads_existing_cache_without_backend_lookup() {
    let expected = CommitMeta::new("cached", 1, "cached subject");
    let mut cache = HashMap::from([("cached".to_string(), expected.clone())]);

    let actual = get_commit_meta(&MetadataBackendStub::failing(), "cached", &mut cache).unwrap();

    assert_eq!(actual, expected);
}

#[test]
fn get_commit_meta_propagates_cache_hydration_errors() {
    let mut cache = HashMap::new();

    let error =
        get_commit_meta(&MetadataBackendStub::failing(), "missing", &mut cache).unwrap_err();

    assert!(matches!(error, GitLsError::TestFixture(_)));
    assert!(cache.is_empty());
}

#[test]
fn get_commit_meta_reports_backends_that_do_not_populate_cache() {
    let mut cache = HashMap::new();

    let error = get_commit_meta(&MetadataBackendStub::empty(), "missing", &mut cache).unwrap_err();

    assert!(matches!(
        error,
        GitLsError::UnexpectedGitShow { oid } if oid == "missing"
    ));
}

#[test]
fn shell_branchless_queries_build_visible_and_hidden_commands() {
    let git = MockGit::default()
        .with(&["branchless", "query", "-r", "roots"], " one\n\n two \n")
        .with(
            &["branchless", "query", "-r", "--hidden", "roots"],
            "hidden-one\n",
        )
        .with(&["branchless", "query", "-b", "roots"], " feature/a \n")
        .with(
            &["branchless", "query", "-b", "--hidden", "roots"],
            "hidden/a\n",
        );

    assert_eq!(
        BranchlessQueries::query_revset(&git, "roots", false).unwrap(),
        vec!["one".to_string(), "two".to_string()]
    );
    assert_eq!(
        BranchlessQueries::query_revset(&git, "roots", true).unwrap(),
        vec!["hidden-one".to_string()]
    );
    assert_eq!(
        BranchlessQueries::query_branch_names(&git, "roots", false).unwrap(),
        vec!["feature/a".to_string()]
    );
    assert_eq!(
        BranchlessQueries::query_branch_names(&git, "roots", true).unwrap(),
        vec!["hidden/a".to_string()]
    );
}

#[test]
fn shell_branchless_queries_propagate_command_errors() {
    let error = BranchlessQueries::query_revset(&MockGit::default(), "roots", false).unwrap_err();
    assert!(matches!(error, GitLsError::TestFixture(_)));

    let error =
        BranchlessQueries::query_branch_names(&MockGit::default(), "roots", false).unwrap_err();
    assert!(matches!(error, GitLsError::TestFixture(_)));
}

#[test]
fn shell_metadata_hydration_skips_fully_cached_aliases() {
    let git = MockGit::default();
    let mut cache = HashMap::new();
    insert_commit_meta(&mut cache, "cached", CommitMeta::new("cached", 1, "cached"));

    shell_cache_commit_metas(&git, &["cached"], &mut cache).unwrap();

    assert!(git.calls().is_empty());
}

#[test]
fn shell_metadata_hydration_propagates_show_errors() {
    let mut cache = HashMap::new();

    let error =
        shell_cache_commit_metas(&MockGit::default(), &["missing"], &mut cache).unwrap_err();

    assert!(matches!(error, GitLsError::TestFixture(_)));
    assert!(cache.is_empty());
}

#[test]
fn shell_metadata_hydration_rejects_record_count_mismatch() {
    let full_oid = "309567f69abcdef0123456789abcdef01234567";
    let git = MockGit::default().with(
        &[
            "show",
            "-s",
            GIT_SHOW_COMMIT_META_ARG,
            "--no-walk=unsorted",
            "one",
            "two",
        ],
        &format!("{full_oid}\x001700000001\x00subject\x1e"),
    );
    let mut cache = HashMap::new();

    let error = shell_cache_commit_metas(&git, &["one", "two"], &mut cache).unwrap_err();

    assert!(matches!(
        error,
        GitLsError::UnexpectedGitShow { oid } if oid == "one, two"
    ));
    assert!(cache.is_empty());
}

#[test]
fn shell_metadata_hydration_propagates_parser_errors() {
    let git = MockGit::default().with(
        &[
            "show",
            "-s",
            GIT_SHOW_COMMIT_META_ARG,
            "--no-walk=unsorted",
            "branch-head",
        ],
        "309567f69abcdef0123456789abcdef01234567\x00soon\x00subject\x1e",
    );
    let mut cache = HashMap::new();

    let error = shell_cache_commit_metas(&git, &["branch-head"], &mut cache).unwrap_err();

    assert!(matches!(
        error,
        GitLsError::InvalidCommitTimestamp { oid, .. } if oid == "branch-head"
    ));
    assert!(cache.is_empty());
}

#[test]
fn shell_metadata_parser_rejects_malformed_records() {
    let error = parse_shell_commit_meta("branch-head", "oid\x001700000001").unwrap_err();

    assert!(matches!(
        error,
        GitLsError::UnexpectedGitShow { oid } if oid == "branch-head"
    ));
}

#[test]
fn shell_metadata_parser_rejects_invalid_timestamps() {
    let error = parse_shell_commit_meta(
        "branch-head",
        "309567f69abcdef0123456789abcdef01234567\x00soon\x00subject",
    )
    .unwrap_err();

    assert!(matches!(
        error,
        GitLsError::InvalidCommitTimestamp { oid, .. } if oid == "branch-head"
    ));
}

fn run_git(repo: &Path, args: &[&str]) -> String {
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

struct TestRepo {
    temp: TempDir,
}

impl TestRepo {
    fn init() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = Self { temp };
        repo.git(&["init", "--initial-branch", "main"]);
        repo.git(&["config", "user.name", "git-ls tests"]);
        repo.git(&["config", "user.email", "git-ls@example.invalid"]);
        repo
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(self.path(), args)
    }

    fn commit_file(&self, path: &str, content: &str, message: &str) -> String {
        std::fs::write(self.path().join(path), content).unwrap();
        self.git(&["add", path]);
        self.git(&["commit", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}

struct AncestryParityFixture {
    repo: TestRepo,
    base_oid: String,
    head_oid: String,
    side_oid: String,
}

fn ancestry_parity_fixture() -> AncestryParityFixture {
    let repo = TestRepo::init();

    repo.commit_file("root.txt", "root\n", "root");
    repo.git(&["checkout", "-b", "side"]);
    let side_oid = repo.commit_file("side.txt", "side\n", "side before base");

    repo.git(&["checkout", "main"]);
    let base_oid = repo.commit_file("base.txt", "base\n", "base");
    repo.git(&["checkout", "-b", "topic"]);
    repo.commit_file("topic.txt", "topic\n", "topic");
    repo.git(&["merge", "--no-ff", "side", "-m", "merge side"]);
    let head_oid = repo.git(&["rev-parse", "HEAD"]);

    AncestryParityFixture {
        repo,
        base_oid,
        head_oid,
        side_oid,
    }
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
fn shell_repository_state_ignores_malformed_branch_records() {
    let git = MockGit::default().with(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:short)",
            "refs/heads",
        ],
        "oid-a\x00feature/a\nmalformed\n",
    );

    let branches = RepositoryStateBackend::local_branches_by_oid(&git).unwrap();

    assert_eq!(branches.len(), 1);
    assert_eq!(branches.get("oid-a"), Some(&vec!["feature/a".to_string()]));
}

#[test]
fn shell_repository_state_propagates_branch_listing_errors() {
    let error = RepositoryStateBackend::local_branches_by_oid(&MockGit::default()).unwrap_err();

    assert!(matches!(error, GitLsError::TestFixture(_)));
}

#[test]
fn shell_repository_state_reads_current_head_branch_variants() {
    let attached = RepositoryCommandStub::output("head-oid\nfeature/a\n");
    assert_eq!(
        RepositoryStateBackend::current_head_and_branch(&attached).unwrap(),
        (Some("head-oid".to_string()), Some("feature/a".to_string()))
    );

    let detached = RepositoryCommandStub::output("head-oid\nHEAD\n");
    assert_eq!(
        RepositoryStateBackend::current_head_and_branch(&detached).unwrap(),
        (Some("head-oid".to_string()), None)
    );

    assert_eq!(
        RepositoryStateBackend::current_head_and_branch(&RepositoryCommandStub::output(""))
            .unwrap(),
        (None, None)
    );
}

#[test]
fn shell_repository_state_reads_configured_and_default_main_branch() {
    let configured = RepositoryCommandStub::output(" trunk \n");

    assert_eq!(
        RepositoryStateBackend::main_branch_name(&configured).unwrap(),
        "trunk"
    );
    assert_eq!(
        RepositoryStateBackend::main_branch_name(&RepositoryCommandStub::output("")).unwrap(),
        "main"
    );
}

#[test]
fn shell_repository_state_propagates_allow_failure_command_errors() {
    let error = RepositoryStateBackend::current_head_and_branch(&RepositoryCommandStub::failing())
        .unwrap_err();
    assert!(matches!(error, GitLsError::TestFixture(_)));

    let error =
        RepositoryStateBackend::main_branch_name(&RepositoryCommandStub::failing()).unwrap_err();
    assert!(matches!(error, GitLsError::TestFixture(_)));

    let error =
        AncestryBackend::merge_base(&RepositoryCommandStub::failing(), "main", "head").unwrap_err();
    assert!(matches!(error, GitLsError::TestFixture(_)));
}

#[test]
fn shell_merge_base_reads_present_and_absent_common_ancestors() {
    let git = RepositoryCommandStub::output("base\n");

    assert_eq!(
        AncestryBackend::merge_base(&git, "main", "head").unwrap(),
        Some("base".to_string())
    );
    assert_eq!(
        AncestryBackend::merge_base(&RepositoryCommandStub::output(""), "main", "head").unwrap(),
        None
    );
}

#[test]
fn shell_ancestry_path_reads_unbounded_head_history() {
    let git = MockGit::default().with(&["rev-list", "--reverse", "head-oid"], "root\nhead-oid\n");

    let path = AncestryBackend::ancestry_path(&git, None, "head-oid").unwrap();

    assert_eq!(path, vec!["root".to_string(), "head-oid".to_string()]);
}

#[test]
fn shell_ancestry_path_propagates_unbounded_history_errors() {
    let error = AncestryBackend::ancestry_path(&MockGit::default(), None, "head").unwrap_err();

    assert!(matches!(error, GitLsError::TestFixture(_)));
}

#[test]
fn shell_ancestry_path_propagates_bounded_history_errors() {
    let error =
        AncestryBackend::ancestry_path(&MockGit::default(), Some("base"), "head").unwrap_err();

    assert!(matches!(error, GitLsError::TestFixture(_)));
}

#[test]
fn gix_backend_matches_git_ancestry_path() {
    let fixture = ancestry_parity_fixture();
    let backend = GixBackend::discover_from(fixture.repo.path()).unwrap();
    let shell_path = lines(&fixture.repo.git(&[
        "rev-list",
        "--reverse",
        "--ancestry-path",
        &format!("{}..{}", fixture.base_oid, fixture.head_oid),
    ]));

    assert_eq!(
        backend
            .merge_base(&fixture.base_oid, &fixture.head_oid)
            .unwrap(),
        Some(fixture.base_oid.clone())
    );
    assert_eq!(
        backend
            .ancestry_path(Some(&fixture.base_oid), &fixture.head_oid)
            .unwrap(),
        shell_path
    );
    assert!(!shell_path.contains(&fixture.side_oid));
}

#[test]
fn gix_backend_reads_repository_snapshot_and_commit_metadata() {
    let repo = TestRepo::init();
    repo.commit_file("root.txt", "root\n", "root");
    repo.git(&["checkout", "-b", "topic"]);
    let head_oid = repo.commit_file("topic.txt", "topic\n", "topic");
    repo.git(&["config", "branchless.core.mainBranch", "trunk"]);

    let backend = GixBackend::discover_from(repo.path()).unwrap();
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
    assert_eq!(meta.subject, "topic");
}

#[test]
fn gix_metadata_hydration_rejects_malformed_oid() {
    let error = GixBackend::object_id("not-an-oid").unwrap_err();

    assert!(matches!(
        error,
        GitLsError::InvalidObjectId { oid, .. } if oid == "not-an-oid"
    ));
}

#[test]
fn gix_metadata_hydration_reports_missing_commits() {
    let repo = TestRepo::init();
    let backend = GixBackend::discover_from(repo.path()).unwrap();
    let mut cache = HashMap::new();

    let error = backend
        .cache_commit_metas(&["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"], &mut cache)
        .unwrap_err();

    assert!(matches!(
        error,
        GitLsError::Gix {
            context: "find commit",
            ..
        }
    ));
    assert!(cache.is_empty());
}

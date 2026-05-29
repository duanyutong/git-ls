use std::collections::HashMap;
use std::path::Path;
use std::process::Command as TestCommand;

use tempfile::TempDir;

use super::metadata::{
    GIT_SHOW_COMMIT_META_ARG, insert_commit_meta, missing_commit_aliases, parse_shell_commit_meta,
};
use super::shell::{lines, shell_cache_commit_metas};
use super::*;
use crate::error::GitLsError;
use crate::model::{CommitMeta, display_short_oid};
use crate::test_support::MockGit;

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

use crate::repo_support::TestRepo;
use std::collections::HashMap;

use super::metadata::{
    GIT_SHOW_COMMIT_META_ARG, insert_commit_meta, missing_commit_aliases, parse_shell_commit_meta,
};
use super::shell::{lines, shell_cache_commit_metas};
use super::*;
use crate::error::{GitLsError, Result};
use crate::model::{CommitMeta, display_short_oid};
use crate::test_support::MockGit;

#[test]
fn batch_configuration_preserves_records_and_last_value_precedence() {
    let values = super::shell::parse_config_records(
        "git-ls.backend\ngix\0git-ls.backend\nshell\0git-ls.cache\0git-ls.other\nline one\nline two\0git-ls.palette\n\0",
    );
    assert_eq!(values["git-ls.backend"], "shell");
    assert_eq!(values["git-ls.cache"], "true");
    assert_eq!(values["git-ls.other"], "line one\nline two");
    assert_eq!(values["git-ls.palette"], "");
}

struct RepoGit<'a>(&'a TestRepo);

impl GitCommand for RepoGit<'_> {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let output = self.0.command("git").args(args).output().unwrap();
        if !(output.status.success() || allow_failure && output.status.code() == Some(1)) {
            return Err(GitLsError::git_command(
                args.join(" "),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(String::from_utf8(output.stdout).unwrap().trim().to_string())
    }
    fn run_input(&self, args: &[&str], input: &str) -> Result<String> {
        let output = self
            .0
            .command("git")
            .args(args)
            .write_stdin(input)
            .assert()
            .success();
        Ok(String::from_utf8(output.get_output().stdout.clone()).unwrap())
    }
}

#[test]
fn batched_queries_match_git_on_nested_and_incomparable_bases() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let a = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "a"]);
    let b = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "b"]);
    let c = repo.git(&["commit-tree", &tree, "-p", &a, "-m", "c"]);
    let merge = repo.git(&[
        "commit-tree",
        &tree,
        "-p",
        &c,
        "-p",
        &b,
        "-p",
        &a,
        "-m",
        "merge",
    ]);
    let orphan = repo.git(&["commit-tree", &tree, "-m", "orphan"]);
    let bases = vec![
        root.clone(),
        a.clone(),
        b,
        c.clone(),
        merge.clone(),
        orphan.clone(),
    ];
    for (i, id) in bases.iter().enumerate() {
        repo.git(&["branch", &format!("tip-{i}"), id]);
    }
    let native = GixBackend::discover_from(repo.path()).unwrap();
    let shell = RepoGit(&repo);
    for backend in [&native as &dyn GitBackend, &shell as &dyn GitBackend] {
        for requested in [&bases[..5], &bases[..], &[]] {
            let distances = backend.main_distances(&merge, requested).unwrap();
            for base in requested {
                let count: usize = repo
                    .git(&[
                        "rev-list",
                        "--count",
                        "--ancestry-path",
                        &format!("{base}..{merge}"),
                    ])
                    .parse()
                    .unwrap();
                assert_eq!(
                    distances[base],
                    if base == &merge {
                        Some(0)
                    } else {
                        (count > 0).then_some(count)
                    }
                );
            }
        }
        let unmerged = backend.unmerged_tips(&a, &bases).unwrap();
        assert!(!unmerged.contains(&root));
        assert!(!unmerged.contains(&a));
        let mut expected = vec![merge.clone(), orphan.clone()];
        expected.sort();
        assert_eq!(backend.independent_heads(&a, &unmerged).unwrap(), expected);
        assert_eq!(
            backend
                .main_distances(&c, &[root.clone(), a.clone(), c.clone()])
                .unwrap(),
            HashMap::from([
                (root.clone(), Some(2)),
                (a.clone(), Some(1)),
                (c.clone(), Some(0))
            ])
        );
    }
}

#[test]
fn shell_large_head_selection_uses_one_shared_stdin_walk() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let mut tips: Vec<String> = Vec::new();
    for i in 0..130 {
        tips.push(repo.git(&[
            "commit-tree",
            &tree,
            "-p",
            tips.last().unwrap_or(&root),
            "-m",
            &format!("commit-{i}"),
        ]));
    }
    let shell = RepoGit(&repo);
    assert_eq!(
        shell.independent_heads(&root, &tips).unwrap(),
        vec![tips.last().unwrap().clone()]
    );
}

#[test]
fn native_distance_respects_shallow_roots_even_when_parent_objects_exist() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let boundary = repo.commit("boundary");
    let head = repo.commit("head");
    std::fs::write(repo.path().join(".git/shallow"), format!("{boundary}\n")).unwrap();
    let native = GixBackend::discover_from(repo.path()).unwrap();
    let shell = RepoGit(&repo);
    let bases = vec![root, boundary];
    assert_eq!(
        native.main_distances(&head, &bases).unwrap(),
        shell.main_distances(&head, &bases).unwrap()
    );
}

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

    assert_eq!(git.calls(), Vec::<Vec<String>>::new());
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

struct AncestryParityFixture {
    repo: TestRepo,
    base_oid: String,
    head_oid: String,
    side_oid: String,
}

fn ancestry_parity_fixture() -> AncestryParityFixture {
    let repo = TestRepo::new();

    repo.commit("root");
    repo.git(&["checkout", "-b", "side"]);
    let side_oid = repo.commit("side before base");

    repo.git(&["checkout", "main"]);
    let base_oid = repo.commit("base");
    repo.git(&["checkout", "-b", "topic"]);
    repo.commit("topic");
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
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "-b", "topic"]);
    let head_oid = repo.commit("topic");
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
    let repo = TestRepo::new();
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

#[test]
fn native_ancestry_matches_git_with_redundant_parents_in_either_order() {
    let repo = TestRepo::new();
    let base = repo.commit("base");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let y = repo.git(&["commit-tree", &tree, "-p", &base, "-m", "y"]);
    let x = repo.git(&["commit-tree", &tree, "-p", &y, "-m", "x"]);
    for parents in [[&x, &y], [&y, &x]] {
        let head = repo.git(&[
            "commit-tree",
            &tree,
            "-p",
            parents[0],
            "-p",
            parents[1],
            "-m",
            "tip",
        ]);
        let expected = lines(&repo.git(&[
            "rev-list",
            "--reverse",
            "--ancestry-path",
            &format!("{base}..{head}"),
        ]));
        let backend = GixBackend::discover_from(repo.path()).unwrap();
        assert_eq!(backend.ancestry_path(Some(&base), &head).unwrap(), expected);
        assert!(expected.contains(&x));
        assert!(expected.contains(&y));
    }
}

#[test]
fn native_ancestry_excludes_old_side_history_and_handles_unrelated_ranges() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let base = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "base"]);
    let feature = repo.git(&["commit-tree", &tree, "-p", &base, "-m", "feature"]);
    let side = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "side"]);
    let head = repo.git(&[
        "commit-tree",
        &tree,
        "-p",
        &feature,
        "-p",
        &side,
        "-m",
        "merge",
    ]);
    let orphan = repo.git(&["commit-tree", &tree, "-m", "orphan"]);
    let backend = GixBackend::discover_from(repo.path()).unwrap();
    assert_eq!(
        backend.ancestry_path(Some(&base), &head).unwrap(),
        vec![feature, head.clone()]
    );
    assert_eq!(
        backend.ancestry_path(Some(&orphan), &head).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        backend.ancestry_path(Some(&head), &head).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(backend.ancestry_path(None, &orphan).unwrap(), vec![orphan]);
}

#[test]
fn native_selection_proves_unique_and_absent_merge_bases() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let main = repo.commit("main");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let a = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "a"]);
    let b = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "b"]);
    let merged = repo.git(&["commit-tree", &tree, "-p", &a, "-p", &b, "-m", "merge"]);
    let current = repo.git(&["commit-tree", &tree, "-p", &main, "-m", "current"]);
    let orphan = repo.git(&["commit-tree", &tree, "-m", "orphan"]);
    let native = GixBackend::discover_from(repo.path()).unwrap();
    let tips = vec![merged.clone(), current.clone(), orphan.clone()];
    native.unmerged_tips(&main, &tips).unwrap();
    assert_eq!(native.merge_base(&main, &merged).unwrap(), Some(root));
    assert_eq!(
        native.merge_base(&main, &current).unwrap(),
        Some(main.clone())
    );
    assert_eq!(native.merge_base(&main, &orphan).unwrap(), None);
    // Directional and unrelated queries cannot consume another selection's proof.
    assert_eq!(native.merge_base(&current, &main).unwrap(), Some(main));
    assert_eq!(native.merge_base(&orphan, &merged).unwrap(), None);
}

#[test]
fn batched_shell_queries_reject_failed_or_malformed_input() {
    let empty = MockGit::default();
    assert!(
        MockGit::default()
            .with_error(&["merge-base", "one", "two"])
            .main_distances("main", &["one".to_string(), "two".to_string()])
            .is_err()
    );
    assert_eq!(
        empty.independent_heads("main", &[]).unwrap(),
        Vec::<String>::new()
    );
    assert!(
        empty
            .independent_heads("main", &["tip".to_string()])
            .is_err()
    );
    assert!(
        empty
            .independent_heads("main", &vec!["tip".to_string(); 129])
            .is_err()
    );
    assert!(empty.unmerged_tips("main", &["tip".to_string()]).is_err());
    assert!(
        RepositoryCommandStub::failing()
            .main_distances("main", &["one".to_string(), "two".to_string()])
            .is_err()
    );
    assert!(empty.main_distances("main", &["base".to_string()]).is_err());
    let invalid_vertex = MockGit::default().with(
        &["rev-list", "--parents", "main", "--not", "base"],
        "invalid",
    );
    assert!(
        invalid_vertex
            .main_distances("main", &["base".to_string()])
            .is_err()
    );
    let invalid_base =
        MockGit::default().with(&["rev-list", "--parents", "main", "--not", "invalid"], "\n");
    assert!(
        invalid_base
            .main_distances("main", &["invalid".to_string()])
            .is_err()
    );
    let no_common = MockGit::default().with(&["rev-list", "--parents", "main"], "");
    assert!(
        no_common
            .main_distances("main", &["a".to_string(), "b".to_string(), "c".to_string()])
            .is_err()
    );
}

#[test]
fn multiple_best_merge_bases_preserve_backend_choice_with_directional_reuse() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let tree = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let a = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "a"]);
    let b = repo.git(&["commit-tree", &tree, "-p", &root, "-m", "b"]);
    let main = repo.git(&["commit-tree", &tree, "-p", &a, "-p", &b, "-m", "main"]);
    let tip = repo.git(&["commit-tree", &tree, "-p", &b, "-p", &a, "-m", "tip"]);
    let expected = lines(&repo.git(&["merge-base", "--all", &main, &tip]));
    assert_eq!(expected.len(), 2);
    let native = GixBackend::discover_from(repo.path()).unwrap();
    let original = native.merge_base(&main, &tip).unwrap();
    native
        .unmerged_tips(&main, std::slice::from_ref(&tip))
        .unwrap();
    assert_eq!(native.merge_base(&main, &tip).unwrap(), original);
    let shell = RepoGit(&repo);
    for backend in [&native as &dyn GitBackend, &shell as &dyn GitBackend] {
        let session = Session::new(backend);
        let selected = session.merge_base(&main, &tip).unwrap().unwrap();
        assert!(expected.contains(&selected));
        assert_eq!(session.merge_base(&main, &tip).unwrap(), Some(selected));
        assert!(expected.contains(&session.merge_base(&tip, &main).unwrap().unwrap()));
    }
}

#[test]
fn batched_shell_adapters_handle_shared_ranges_and_empty_boundaries() {
    let root = "1".repeat(40);
    let middle = "2".repeat(40);
    let main = "3".repeat(40);
    let orphan = "4".repeat(40);
    let git = MockGit::default()
        .with(&["merge-base", &root, &middle], &root)
        .with(
            &["rev-list", "--parents", &main],
            &format!("{main} {middle}\n{middle} {root}\n{root}\n\n"),
        );
    let expected = HashMap::from([
        (root.clone(), Some(2)),
        (middle.clone(), Some(1)),
        (main.clone(), Some(0)),
        (orphan.clone(), None),
    ]);
    for bases in [
        vec![root.clone(), middle.clone(), orphan.clone(), main.clone()],
        vec![orphan, root, middle, main.clone()],
    ] {
        assert_eq!(git.main_distances(&main, &bases).unwrap(), expected);
    }
    let tips: Vec<_> = (0..129).map(|i| format!("tip-{i}")).collect();
    let parents = (1..129)
        .map(|i| format!("tip-{i} tip-{}", i - 1))
        .collect::<Vec<_>>()
        .join("\n");
    let git = MockGit::default().with(&["rev-list", "--parents", "--stdin"], &parents);
    assert_eq!(
        git.independent_heads("main", &tips).unwrap(),
        vec!["tip-128"]
    );
    assert!(
        RepositoryCommandStub::failing()
            .independent_heads("main", &tips)
            .is_err()
    );
}

#[test]
fn shell_distance_batch_preserves_sha256_repository_support() {
    let repo = TestRepo::new();
    std::fs::remove_dir_all(repo.path().join(".git")).unwrap();
    repo.git(&[
        "-c",
        "init.templateDir=",
        "init",
        "--initial-branch=main",
        "--object-format=sha256",
    ]);
    repo.git(&["config", "user.name", "Example"]);
    repo.git(&["config", "user.email", "example@example.invalid"]);
    repo.git(&["config", "commit.gpgSign", "false"]);
    let root = repo.commit("root");
    let middle = repo.commit("middle");
    let main = repo.commit("main");
    assert_eq!(main.len(), 64);
    let shell = RepoGit(&repo);
    let bases = vec![root.clone(), middle.clone(), main.clone()];
    assert_eq!(
        shell.main_distances(&main, &bases).unwrap(),
        HashMap::from([(root, Some(2)), (middle, Some(1)), (main, Some(0))])
    );
}

#[test]
fn shell_metadata_batch_bounds_command_size() {
    let ids: Vec<_> = (0..300).map(|i| format!("{i:040x}")).collect();
    let mut git = MockGit::default();
    for chunk in ids.chunks(256) {
        let mut args = vec!["show", "-s", GIT_SHOW_COMMIT_META_ARG, "--no-walk=unsorted"];
        args.extend(chunk.iter().map(String::as_str));
        let mut output = String::new();
        for id in chunk {
            output.push_str(id);
            output.push_str("\x001\x00subject\x1e");
        }
        git = git.with(&args, &output);
    }
    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut cache = HashMap::new();
    git.cache_commit_metas(&refs, &mut cache).unwrap();
    assert_eq!(cache.len(), 300);
    assert_eq!(git.calls().len(), 2);
    assert!(git.calls().iter().all(|args| args.len() <= 260));
}

#[test]
fn selection_uses_captured_ids_after_refs_move() {
    let repo = TestRepo::new();
    let main = repo.commit("main");
    repo.git(&["checkout", "-b", "topic"]);
    let tip = repo.commit("topic");
    repo.git(&["checkout", "main"]);
    repo.git(&["branch", "-D", "topic"]);
    let native = GixBackend::discover_from(repo.path()).unwrap();
    let shell = RepoGit(&repo);
    for backend in [&native as &dyn GitBackend, &shell as &dyn GitBackend] {
        assert_eq!(
            backend
                .unmerged_tips(&main, std::slice::from_ref(&tip))
                .unwrap(),
            vec![tip.clone()]
        );
    }
}

#[test]
fn shell_selection_ignores_unlabelled_internal_commits() {
    let git = MockGit::default().with(&["rev-list", "--stdin"], "tip\ninternal\n");
    assert_eq!(
        git.unmerged_tips("main", &["tip".to_string()]).unwrap(),
        vec!["tip"]
    );
}

use super::*;
use crate::repo_support::TestRepo;

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

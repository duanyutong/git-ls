#![allow(clippy::expect_used)]

mod support;
use support::TestRepo;

fn write(repo: &TestRepo, name: &str, content: impl AsRef<[u8]>) {
    let path = repo.path().join(name);
    std::fs::create_dir_all(path.parent().expect("fixture directory")).expect("fixture directory");
    std::fs::write(path, content).expect("fixture file");
}

fn run(repo: &TestRepo, backend: &str, options: &[&str]) -> (String, String) {
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .args([
            "--backend",
            backend,
            "--layout",
            "columns",
            "--color",
            "never",
            "--debug",
        ])
        .args(options)
        .assert()
        .success();
    (
        String::from_utf8(output.get_output().stdout.clone()).expect("output"),
        String::from_utf8(output.get_output().stderr.clone()).expect("diagnostics"),
    )
}

fn row<'a>(output: &'a str, branch: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.contains(branch))
        .expect("branch row")
}

#[test]
fn measures_combined_layer_changes_with_renames_binary_files_and_shared_ancestry() {
    let repo = TestRepo::new();
    write(&repo, "data", "one\ntwo\nthree\n");
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/base"]);
    write(&repo, "data", "changed\ntwo\nthree\nfour\n");
    repo.commit("temporary change");
    write(&repo, "data", "one\ntwo\nthree\nfour\n");
    repo.commit("retain only final change");
    repo.git(&["checkout", "-b", "feature/child"]);
    write(&repo, "data", "one\nthree\nfour\n");
    repo.commit("remove second line");
    repo.git(&["branch", "feature/alias"]);
    repo.git(&["checkout", "-b", "feature/rename", "feature/base"]);
    repo.git(&["mv", "data", "renamed\tfile\nname"]);
    write(&repo, "binary", b"\0new binary\0");
    repo.commit("rename and add binary");
    repo.git(&["checkout", "-b", "feature/edited", "feature/base"]);
    repo.git(&["mv", "data", "edited"]);
    write(&repo, "edited", "one\ntwo\nthree\nfour\nfive\n");
    repo.commit("edit a renamed file");
    std::fs::create_dir(repo.path().join("nested")).expect("nested directory");
    repo.git(&["config", "diff.relative", "true"]);
    for backend in ["gix", "shell"] {
        let (output, _) = run(&repo, backend, &["-vv"]);
        assert!(row(&output, "feature/base").contains("+1 -0"), "{output}");
        assert!(row(&output, "feature/child").contains("+0 -1"), "{output}");
        assert!(row(&output, "feature/rename").contains("+0 -0"), "{output}");
        assert!(row(&output, "feature/edited").contains("+1 -0"), "{output}");
        assert_eq!(
            output.matches("retain only final change").count(),
            1,
            "{output}"
        );
        let (cached, debug) = run(&repo, backend, &["-vv"]);
        assert!(debug.contains("cache=hit"), "{debug}");
        assert!(!debug.contains("operation=subprocess_diff"), "{debug}");
        assert!(debug.contains("diff_cache_hits=4"), "{debug}");
        assert_eq!(cached, output);
        assert_eq!(cached, run(&repo, backend, &["-vv", "--no-cache"]).0);
        let (plain, debug) = run(&repo, backend, &["-vv", "--no-diff-stat"]);
        assert!(!plain.contains("+1 -0"), "{plain}");
        assert!(!debug.contains("operation=diff_stat"), "{debug}");
        assert!(plain.contains("retain only final change"));
        let nested = repo
            .command(assert_cmd::cargo::cargo_bin!("git-ls"))
            .current_dir(repo.path().join("nested"))
            .args(["--backend", backend, "--color", "never", "-vv"])
            .assert()
            .success();
        let nested_output = String::from_utf8(nested.get_output().stdout.clone()).expect("output");
        assert!(
            row(&nested_output, "feature/base").contains("+1 -0"),
            "{nested_output}"
        );
    }
}

fn assert_count(repo: &TestRepo, backend: &str, expected: &str) {
    let (output, _) = run(repo, backend, &["-vv"]);
    assert!(row(&output, "feature/topic").contains(expected), "{output}");
    assert_eq!(output, run(repo, backend, &["-vv", "--no-cache"]).0);
    let (warm, debug) = run(repo, backend, &["-vv"]);
    assert_eq!(warm, output);
    assert!(!debug.contains("operation=subprocess_diff"), "{debug}");
}

#[test]
fn invalidates_statistics_for_effective_worktree_index_info_and_global_attributes() {
    for backend in ["gix", "shell"] {
        let repo = TestRepo::new();
        repo.commit("root");
        repo.git(&["checkout", "-b", "feature/topic"]);
        write(&repo, "data", "line\n");
        repo.commit("topic");
        assert_count(&repo, backend, "+1 -0");
        write(&repo, ".gitattributes", "data -diff\n");
        assert_count(&repo, backend, "+0 -0");
        repo.git(&["add", ".gitattributes"]);
        std::fs::remove_file(repo.path().join(".gitattributes"))
            .expect("remove working attributes");
        assert_count(&repo, backend, "+0 -0");
        repo.git(&["rm", "--cached", ".gitattributes"]);
        assert_count(&repo, backend, "+1 -0");
        write(&repo, ".git/info/attributes", "data -diff\n");
        assert_count(&repo, backend, "+0 -0");
        write(&repo, ".git/info/attributes", "");
        assert_count(&repo, backend, "+1 -0");
        write(&repo, "global-attributes", "data -diff\n");
        repo.git(&[
            "config",
            "core.attributesFile",
            repo.path()
                .join("global-attributes")
                .to_str()
                .expect("attribute path"),
        ]);
        assert_count(&repo, backend, "+0 -0");
        write(&repo, "global-attributes", "data diff=custom\n");
        repo.git(&["config", "diff.custom.binary", "true"]);
        assert_count(&repo, backend, "+0 -0");
        repo.git(&["config", "diff.custom.binary", "false"]);
        assert_count(&repo, backend, "+1 -0");
    }
}

#[test]
fn shares_graph_facts_but_keeps_attribute_sensitive_statistics_worktree_specific() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/topic"]);
    write(&repo, "data", "line\n");
    repo.commit("topic");
    let linked = tempfile::tempdir().expect("linked worktree directory");
    repo.git(&[
        "worktree",
        "add",
        "--detach",
        linked.path().to_str().expect("worktree path"),
        "HEAD",
    ]);
    std::fs::write(linked.path().join(".gitattributes"), "data -diff\n")
        .expect("linked attributes");
    for backend in ["gix", "shell"] {
        assert_count(&repo, backend, "+1 -0");
        let linked_output = repo
            .command(assert_cmd::cargo::cargo_bin!("git-ls"))
            .current_dir(linked.path())
            .args(["--backend", backend, "--color", "never", "-vv"])
            .assert()
            .success();
        let output = String::from_utf8_lossy(&linked_output.get_output().stdout);
        assert!(row(&output, "feature/topic").contains("+0 -0"), "{output}");
        assert_count(&repo, backend, "+1 -0");
    }
}

#[test]
fn batches_many_layers_and_skips_equal_trees_without_changing_endpoint_totals() {
    let repo = TestRepo::new();
    repo.commit("root");
    for index in 0..8 {
        repo.git(&["checkout", "-b", &format!("feature/{index}"), "main"]);
        write(&repo, "data", format!("{index}\n"));
        repo.commit("change");
    }
    repo.git(&["checkout", "-b", "feature/empty", "main"]);
    repo.commit("empty");
    for backend in ["gix", "shell"] {
        let (_, debug) = run(&repo, backend, &["-vv", "--no-cache"]);
        assert!(debug.contains("diff_batch_launches=1"), "{debug}");
        assert!(debug.contains("diff_equal_tree_skips=1"), "{debug}");
        run(&repo, backend, &["-vv", "--refresh-cache"]);
        let (_, debug) = run(&repo, backend, &["-vv"]);
        assert!(debug.contains("diff_cache_hits=9"), "{debug}");
        assert!(!debug.contains("operation=subprocess_diff"), "{debug}");
        assert!(!debug.contains("stage=cache_revalidation"), "{debug}");
    }
}

#[test]
fn typed_boolean_batch_preserves_effective_values_and_rejects_invalid_ones() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/topic"]);
    write(&repo, "data", "one\n");
    repo.commit("topic");
    for key in ["git-ls.diffStat", "git-ls.cache", "git-ls.cacheMaintenance"] {
        repo.git(&["config", "--add", key, "false"]);
        repo.git(&["config", "--add", key, "true"]);
    }
    assert!(run(&repo, "gix", &["-vv"]).0.contains("+1 -0"));
    repo.git(&["config", "--add", "git-ls.cache", "invalid"]);
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .assert()
        .failure()
        .stdout("");
}

#[test]
fn shell_statistics_support_sha256_roots_and_stacks() {
    let repo = TestRepo::new();
    std::fs::remove_dir_all(repo.path().join(".git")).expect("replace repository");
    repo.git(&[
        "-c",
        "init.templateDir=",
        "init",
        "-b",
        "main",
        "--object-format=sha256",
    ]);
    repo.git(&["config", "user.name", "Example"]);
    repo.git(&["config", "user.email", "example@example.invalid"]);
    repo.git(&["config", "commit.gpgSign", "false"]);
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/topic"]);
    write(&repo, "data", "one\n");
    repo.commit("topic");
    repo.git(&["checkout", "--orphan", "feature/island"]);
    write(&repo, "data", "one\ntwo\n");
    repo.commit("island");
    let (output, debug) = run(&repo, "shell", &["-vv"]);
    assert!(row(&output, "feature/topic").contains("+1 -0"), "{output}");
    assert!(row(&output, "feature/island").contains("+2 -0"), "{output}");
    assert!(debug.contains("diff_batch_launches=1"), "{debug}");
}

#[test]
fn cli_and_boolean_configuration_override_the_verbosity_preset() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/topic"]);
    write(&repo, "data", "one\ntwo\n");
    repo.commit("topic");
    for backend in ["gix", "shell"] {
        for level in ["0", "1", "2"] {
            let (output, debug) = run(&repo, backend, &["--verbosity", level]);
            assert_eq!(output.contains("+2 -0"), level == "2");
            assert_eq!(debug.contains("operation=diff_stat"), level == "2");
        }
        repo.git(&["config", "git-ls.diffStat", "false"]);
        assert!(!run(&repo, backend, &["-vv"]).0.contains("+2 -0"));
        assert!(
            run(&repo, backend, &["--verbosity", "0", "--diff-stat"])
                .0
                .contains("+2 -0")
        );
        repo.git(&["config", "git-ls.diffStat", "2"]);
        assert!(
            run(&repo, backend, &["--verbosity", "0"])
                .0
                .contains("+2 -0")
        );
        assert!(!run(&repo, backend, &["--no-diff-stat"]).0.contains("+2 -0"));
        repo.git(&["config", "git-ls.diffStat", ""]);
        assert!(!run(&repo, backend, &["-vv"]).0.contains("+2 -0"));
        repo.git(&["config", "--unset", "git-ls.diffStat"]);
    }
    repo.git(&["config", "git-ls.diffStat", "invalid"]);
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .assert()
        .failure()
        .stdout("");
}

#[test]
fn unrelated_layers_are_compared_with_the_empty_tree() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "--orphan", "feature/island"]);
    write(&repo, "data", "first\nsecond\n");
    repo.commit("island root");
    write(&repo, "data", "first\nsecond\nthird\n");
    repo.commit("island tip");
    repo.git(&["checkout", "--orphan", "feature/second"]);
    write(&repo, "data", "other\n");
    repo.commit("second island");
    for backend in ["gix", "shell"] {
        let (output, debug) = run(&repo, backend, &["-vv"]);
        assert!(row(&output, "feature/island").contains("+3 -0"), "{output}");
        assert!(row(&output, "feature/second").contains("+1 -0"), "{output}");
        assert!(row(&output, "feature/island").contains("(orphaned)"));
        let hash = debug
            .lines()
            .find(|line| line.contains("operation=subprocess_hash_object"))
            .expect("empty-tree resolution");
        assert!(hash.contains("calls=1"), "{debug}");
    }
}

#[test]
fn compares_merge_endpoints_and_coalesces_distinct_commits_with_identical_trees() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["checkout", "-b", "feature/base"]);
    write(&repo, "data", "one\n");
    let base = repo.commit("base");
    repo.git(&["checkout", "-b", "side", "main"]);
    write(&repo, "side", "two\nthree\n");
    repo.commit("side");
    repo.git(&["checkout", "-b", "feature/merged", "feature/base"]);
    repo.git(&["merge", "--no-ff", "side", "-m", "merge"]);
    repo.git(&["branch", "-D", "side"]);
    repo.git(&["checkout", "-b", "feature/twin", "feature/base"]);
    repo.git(&["commit", "--amend", "-m", "same tree, different commit"]);
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), base);
    for backend in ["gix", "shell"] {
        let (output, debug) = run(&repo, backend, &["-vv", "--no-cache"]);
        assert!(row(&output, "feature/base").contains("+1 -0"), "{output}");
        assert!(row(&output, "feature/twin").contains("+1 -0"), "{output}");
        assert!(row(&output, "feature/merged").contains("+2 -0"), "{output}");
        assert!(debug.contains("diff_unique_pairs=3"), "{debug}");
        assert!(debug.contains("diff_tree_pairs=2"), "{debug}");
    }
}

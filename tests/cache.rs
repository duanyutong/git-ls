#![allow(clippy::expect_used)]
mod support;
use support::TestRepo;

fn run(repo: &TestRepo, options: &[&str]) -> (String, String) {
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .args(["--debug", "--color", "never"])
        .args(options)
        .assert()
        .success();
    (
        String::from_utf8(output.get_output().stdout.clone()).expect("output"),
        String::from_utf8(output.get_output().stderr.clone()).expect("diagnostics"),
    )
}

fn fixture() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["branch", "topic"]);
    repo.commit("main update");
    repo.git(&["checkout", "topic"]);
    repo.commit("topic");
    repo
}

#[test]
fn repeats_reuse_analysis_and_presentation_remains_live() {
    let repo = fixture();
    for backend in ["gix", "shell"] {
        let (_, cold) = run(&repo, &["--backend", backend]);
        assert!(cold.contains("cache=miss"), "{cold}");
        for order in ["newest", "oldest"] {
            for verbosity in ["0", "1", "2"] {
                for layout in ["inline", "columns"] {
                    let options = [
                        "--backend",
                        backend,
                        "--order",
                        order,
                        "--verbosity",
                        verbosity,
                        "--layout",
                        layout,
                    ];
                    let (cached, debug) = run(&repo, &options);
                    assert!(debug.contains("cache=hit"), "{debug}");
                    assert!(!debug.contains("operation=ancestry_path"), "{debug}");
                    assert!(!debug.contains("stage=selection"), "{debug}");
                    let mut uncached = options.to_vec();
                    uncached.push("--no-cache");
                    assert_eq!(cached, run(&repo, &uncached).0);
                }
            }
        }
    }
    repo.git(&["checkout", "main"]);
    let (cached, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=hit"));
    assert_eq!(cached, run(&repo, &["--no-cache"]).0);
    repo.git(&["checkout", "--detach", "topic"]);
    assert_eq!(run(&repo, &[]).0, run(&repo, &["--no-cache"]).0);
}

#[test]
fn changes_recompute_dependent_analysis_and_retain_immutable_facts() {
    let repo = fixture();
    run(&repo, &[]);
    repo.commit("next topic commit");
    let (output, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=miss") || debug.contains("cache=invalid"));
    assert!(debug.contains("distance_cache_hits=1"), "{debug}");
    assert!(debug.contains("persistent_metadata_hits="), "{debug}");
    assert_eq!(output, run(&repo, &["--no-cache"]).0);
    repo.git(&["branch", "-m", "renamed"]);
    let (output, debug) = run(&repo, &[]);
    assert!(!debug.contains("cache=hit"));
    assert!(debug.contains("merge_base_cache_hits="));
    assert!(output.contains("renamed"));
    repo.git(&["checkout", "main"]);
    repo.commit("advance main");
    assert_eq!(run(&repo, &[]).0, run(&repo, &["--no-cache"]).0);
    let (_, refresh) = run(&repo, &["--refresh-cache"]);
    assert!(refresh.contains("reason=refresh"));
    assert!(!refresh.contains("distance_cache_hits="));
}

#[test]
fn corrupt_entries_refresh_clear_and_unsupported_graphs_are_safe() {
    let repo = fixture();
    let (expected, _) = run(&repo, &[]);
    let root = repo.path().join(".git/git-ls/cache/v1");
    for entry in std::fs::read_dir(&root).expect("cache") {
        let entry = entry.expect("entry");
        if entry.file_name().to_string_lossy().starts_with("snapshot-") {
            std::fs::write(entry.path(), "corrupt").expect("corrupt fixture");
        }
    }
    let (output, debug) = run(&repo, &[]);
    assert_eq!(output, expected);
    assert!(debug.contains("cache=invalid"));
    let (_, debug) = run(&repo, &["--clear-cache"]);
    assert!(debug.contains("cache_clear=complete"));
    assert!(!std::fs::read_dir(&root).expect("cache").any(|entry| {
        entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .starts_with("snapshot-")
    }));
    let boundary = repo.git(&["rev-parse", "main"]);
    std::fs::write(repo.path().join(".git/shallow"), format!("{boundary}\n")).expect("shallow");
    let (_, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=bypass reason=unsupported_graph_or_store"));
}

#[test]
fn linked_worktrees_share_cache_without_sharing_current_markers() {
    let repo = fixture();
    run(&repo, &[]);
    let worktree = tempfile::tempdir().expect("worktree");
    repo.git(&[
        "worktree",
        "add",
        "--detach",
        worktree.path().to_str().expect("path"),
        "main",
    ]);
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .current_dir(worktree.path())
        .args(["--debug", "--color", "never"])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&output.get_output().stderr).contains("cache=hit"));
    let uncached = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .current_dir(worktree.path())
        .args(["--no-cache", "--color", "never"])
        .assert()
        .success();
    assert_eq!(output.get_output().stdout, uncached.get_output().stdout);
}

#[test]
fn simultaneous_publishers_produce_complete_readable_entries() {
    let repo = fixture();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                run(&repo, &["--refresh-cache"]);
            });
        }
    });
    let (cached, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=hit"));
    assert_eq!(cached, run(&repo, &["--no-cache"]).0);
}

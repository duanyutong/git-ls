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
        assert!(
            cold.contains("cache=miss") || cold.contains("cache=invalid"),
            "{cold}"
        );
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

#[cfg(unix)]
#[test]
fn maintenance_cannot_hold_output_pipes_open_and_reservations_coalesce() {
    use std::hash::{Hash, Hasher};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let repo = fixture();
    run(&repo, &[]);
    let state_path = repo
        .path()
        .join(".git/git-ls/cache/v1/maintenance-state.json");
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("state")).expect("envelope");
    let payload = r#"{"publications":16,"last_attempt":0}"#;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    payload.hash(&mut hash);
    envelope["payload"] = serde_json::json!(payload);
    envelope["checksum"] = serde_json::json!(hash.finish());
    std::fs::write(&state_path, serde_json::to_vec(&envelope).expect("encode"))
        .expect("seed due state");
    let helper = tempfile::tempdir().expect("helper");
    let signal = helper.path().join("release");
    let started = helper.path().join("started");
    let done = helper.path().join("done");
    let script = helper.path().join("nice");
    std::fs::write(&script, format!(
        "#!/bin/sh\necho started > '{}'\ni=0\nwhile [ ! -f '{}' ] && [ \"$i\" -lt 200 ]; do i=$((i + 1)); sleep 0.05; done\necho worker-output-must-be-silent\necho done > '{}'\nshift 2\nexec \"$@\"\n", started.display(), signal.display(), done.display())).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let path = std::env::join_paths(std::iter::once(helper.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").expect("PATH")),
    ))
    .expect("PATH");
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .env("PATH", &path)
        .args(["--debug", "--color", "never"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&output.get_output().stderr).contains("maintenance=scheduled"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(started.exists());
    assert!(
        !done.exists(),
        "foreground and pipe readers must finish while the worker is blocked"
    );
    let (_, debug) = run(&repo, &[]);
    assert!(debug.contains("maintenance=not_due"));
    std::fs::write(&signal, "release").expect("unblock worker");
    let report = repo
        .path()
        .join(".git/git-ls/cache/v1/maintenance-report.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !report.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(report.exists());
    assert!(!String::from_utf8_lossy(&output.get_output().stdout).contains("worker-output"));
    assert!(run(&repo, &[]).1.contains("maintenance_previous"));
    assert!(
        run(&repo, &["--no-cache-maintenance"])
            .1
            .contains("maintenance=disabled")
    );
}

#[test]
fn configuration_controls_cache_and_only_one_startup_query_is_needed() {
    let repo = fixture();
    repo.git(&["config", "git-ls.cache", "false"]);
    let (_, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=disabled"));
    assert!(
        debug.contains("operation=subprocess_config parent=config calls=1"),
        "{debug}"
    );
    assert!(
        run(&repo, &["--refresh-cache"])
            .1
            .contains("reason=refresh")
    );
    repo.git(&["config", "git-ls.cache", "true"]);
    repo.git(&["config", "git-ls.cacheMaintenance", "false"]);
    let (_, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=hit"));
    assert!(debug.contains("maintenance=disabled"));
    repo.git(&["config", "git-ls.cache", "invalid"]);
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid git config git-ls.cache"));
}

#[test]
fn graph_interpretation_changes_bypass_and_reference_changes_invalidate() {
    let repo = fixture();
    run(&repo, &[]);
    repo.git(&["branch", "alias", "topic"]);
    let (output, debug) = run(&repo, &[]);
    assert!(!debug.contains("cache=hit"));
    assert!(output.contains("alias"));
    repo.git(&["branch", "-D", "alias"]);
    assert_eq!(run(&repo, &[]).0, run(&repo, &["--no-cache"]).0);
    repo.git(&["config", "remote.origin.promisor", "true"]);
    assert!(run(&repo, &[]).1.contains("cache=bypass"));
    repo.git(&["config", "--unset", "remote.origin.promisor"]);
    repo.git(&["replace", "main", "topic"]);
    assert!(run(&repo, &[]).1.contains("cache=bypass"));
    repo.git(&["pack-refs", "--all"]);
    assert!(run(&repo, &[]).1.contains("cache=bypass"));
    repo.git(&["replace", "--delete", "main"]);
    std::fs::create_dir_all(repo.path().join(".git/info")).expect("info directory");
    std::fs::write(repo.path().join(".git/info/grafts"), "").expect("grafts");
    assert!(run(&repo, &[]).1.contains("cache=bypass"));
    std::fs::remove_file(repo.path().join(".git/info/grafts")).expect("remove grafts");
    std::fs::write(repo.path().join(".git/objects/info/alternates"), "").expect("alternates");
    assert!(run(&repo, &[]).1.contains("cache=bypass"));
    std::fs::remove_file(repo.path().join(".git/objects/info/alternates"))
        .expect("remove alternates");
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .args(["--debug", "--color", "never"])
        .assert()
        .success()
        .stderr(predicates::str::contains("cache=bypass"));
}

#[test]
fn unavailable_cache_storage_does_not_prevent_rendering() {
    let repo = fixture();
    std::fs::write(repo.path().join(".git/git-ls"), "unavailable").expect("block cache directory");
    let (actual, debug) = run(&repo, &[]);
    assert!(debug.contains("cache=bypass reason=unavailable"));
    assert_eq!(actual, run(&repo, &["--no-cache"]).0);
}

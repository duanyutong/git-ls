#![allow(clippy::expect_used)]

mod support;
use support::TestRepo;

fn base_command(repo: &TestRepo, backend: &str) -> std::process::Command {
    let mut command = repo.std_command(assert_cmd::cargo::cargo_bin!("git-ls"));
    command.args([
        "--backend",
        backend,
        "--debug",
        "--color",
        "never",
        "-vv",
        "--no-cache-maintenance",
    ]);
    command
}

fn command(repo: &TestRepo, backend: &str) -> assert_cmd::Command {
    base_command(repo, backend).into()
}

fn run(repo: &TestRepo, backend: &str) -> String {
    let output = command(repo, backend).assert().success();
    String::from_utf8_lossy(&output.get_output().stderr).into_owned()
}

#[test]
fn top_only_commits_aliases_and_amendments_preserve_lower_statistics() {
    for backend in ["gix", "shell"] {
        let repo = TestRepo::new();
        repo.commit("root");
        for branch in ["feature/lower", "feature/middle", "feature/top"] {
            repo.git(&["checkout", "-b", branch]);
            std::fs::write(repo.path().join("data"), format!("{branch}\n")).expect("fixture file");
            repo.commit(branch);
        }
        run(&repo, backend);
        assert!(run(&repo, backend).contains("diff_cache_hits=3"));
        std::fs::write(repo.path().join("data"), "next top\n").expect("fixture file");
        repo.commit("top only");
        let changed = run(&repo, backend);
        assert!(changed.contains("diff_cache_hits=2"), "{changed}");
        assert!(changed.contains("diff_cache_misses=1"), "{changed}");
        repo.git(&["branch", "feature/alias"]);
        let alias = run(&repo, backend);
        assert!(alias.contains("diff_cache_hits=3"), "{alias}");
        assert!(!alias.contains("operation=subprocess_diff"), "{alias}");
        repo.git(&["branch", "-d", "feature/alias"]);
        repo.git(&["commit", "--amend", "-m", "metadata only"]);
        let amended = run(&repo, backend);
        assert!(amended.contains("diff_tree_cache_hits=3"), "{amended}");
        assert!(!amended.contains("operation=subprocess_diff"), "{amended}");
    }
}

#[cfg(unix)]
struct GitBarrier {
    _directory: tempfile::TempDir,
    ready: std::path::PathBuf,
    release: std::path::PathBuf,
    path: std::ffi::OsString,
    real_git: std::path::PathBuf,
}

#[cfg(unix)]
impl GitBarrier {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().expect("wrapper directory");
        let ready = directory.path().join("ready");
        let release = directory.path().join("release");
        let wrapper = directory.path().join("git");
        let original_path = std::env::var_os("PATH").expect("PATH");
        let real_git = std::env::split_paths(&original_path)
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .expect("Git executable");
        std::fs::write(
            &wrapper,
            r#"#!/bin/sh
if [ "$1" = diff-tree ] && [ "$LS_TEST_PAUSE" = 1 ] && [ ! -e "$LS_TEST_READY" ]; then
    : > "$LS_TEST_READY"
    i=0
    while [ ! -e "$LS_TEST_RELEASE" ] && [ "$i" -lt 600 ]; do
        i=$((i + 1))
        sleep 0.1
    done
fi
exec "$LS_TEST_REAL_GIT" "$@"
"#,
        )
        .expect("Git wrapper");
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
            .expect("executable wrapper");
        let path = std::env::join_paths(
            std::iter::once(directory.path().to_path_buf())
                .chain(std::env::split_paths(&original_path)),
        )
        .expect("wrapper PATH");
        Self {
            _directory: directory,
            ready,
            release,
            path,
            real_git,
        }
    }

    fn process(&self, repo: &TestRepo, backend: &str, pause: bool) -> std::process::Command {
        let mut invocation = base_command(repo, backend);
        invocation
            .env("PATH", &self.path)
            .env("LS_TEST_REAL_GIT", &self.real_git)
            .env("LS_TEST_PAUSE", if pause { "1" } else { "0" })
            .env("LS_TEST_READY", &self.ready)
            .env("LS_TEST_RELEASE", &self.release);
        invocation
    }

    fn command(&self, repo: &TestRepo, backend: &str, pause: bool) -> assert_cmd::Command {
        self.process(repo, backend, pause).into()
    }

    fn wait_ready(&self, child: &mut std::process::Child) -> bool {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(60);
        while !self.ready.exists() && Instant::now() < deadline {
            if child.try_wait().expect("first process status").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.ready.exists()
    }

    fn release(&self) {
        std::fs::write(&self.release, "release").expect("release wrapper");
    }
}

#[cfg(unix)]
impl Drop for GitBarrier {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "release");
    }
}

#[cfg(unix)]
fn retained_comparisons(repo: &TestRepo, backend: &str) -> usize {
    let file = repo
        .path()
        .join(format!(".git/git-ls/cache/v1/facts-{backend}.json"));
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(file).expect("facts entry")).expect("facts envelope");
    let facts: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().expect("payload"))
            .expect("facts payload");
    let scopes = facts["diff"]["scopes"].as_object().expect("contexts");
    assert_eq!(scopes.len(), 1);
    scopes.values().next().expect("context")["values"]
        .as_object()
        .expect("values")
        .len()
}

#[cfg(unix)]
#[test]
fn reference_movement_retains_immutable_facts_and_merges_a_later_writer() {
    use std::process::Stdio;
    use std::time::Duration;
    for backend in ["gix", "shell"] {
        let repo = TestRepo::new();
        repo.commit("root");
        repo.git(&["checkout", "-b", "feature/topic"]);
        std::fs::write(repo.path().join("data"), "one\n").expect("fixture file");
        let first_tip = repo.commit("first tip");
        std::fs::write(repo.path().join("data"), "one\ntwo\n").expect("fixture file");
        let second_tip = repo.commit("second tip");
        repo.git(&["update-ref", "refs/heads/feature/topic", &first_tip]);
        let wrapper = GitBarrier::new();
        let mut child = wrapper
            .process(&repo, backend, true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("first invocation");
        if !wrapper.wait_ready(&mut child) {
            wrapper.release();
            let _ = child.kill();
            let output = child.wait_with_output().expect("first output");
            panic!(
                "first invocation did not reach barrier: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        repo.git(&["update-ref", "refs/heads/feature/topic", &second_tip]);
        let second = wrapper
            .command(&repo, backend, false)
            .timeout(Duration::from_secs(60))
            .output();
        wrapper.release();
        let first = child.wait_with_output().expect("first output");
        let second = second.expect("second output");
        assert!(
            second.status.success(),
            "{}",
            String::from_utf8_lossy(&second.stderr)
        );
        let debug = String::from_utf8_lossy(&first.stderr);
        assert!(first.status.success(), "{debug}");
        assert!(
            debug.contains("cache_publication=discarded reason=inputs_changed entry=snapshot"),
            "{debug}"
        );
        assert!(debug.contains("cache_facts_merge entries="), "{debug}");
        assert_eq!(retained_comparisons(&repo, backend), 2);
        for tip in [&first_tip, &second_tip] {
            repo.git(&["update-ref", "refs/heads/feature/topic", tip]);
            let output = wrapper.command(&repo, backend, false).assert().success();
            let debug = String::from_utf8_lossy(&output.get_output().stderr);
            assert!(debug.contains("diff_cache_hits=1"), "{debug}");
            assert!(!debug.contains("operation=subprocess_diff"), "{debug}");
        }
    }
}

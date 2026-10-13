#![allow(clippy::expect_used)]
mod support;
use support::TestRepo;

fn fixture() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["branch", "topic"]);
    repo.commit("main update");
    repo.git(&["checkout", "topic"]);
    repo.commit("topic update");
    repo
}

fn output(repo: &TestRepo, backend: &str) -> (String, String) {
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .args([
            "--backend",
            backend,
            "--debug",
            "--verbosity",
            "2",
            "--color",
            "never",
        ])
        .assert()
        .success();
    (
        String::from_utf8(output.get_output().stdout.clone()).expect("UTF-8 output"),
        String::from_utf8(output.get_output().stderr.clone()).expect("UTF-8 diagnostics"),
    )
}

#[test]
fn altered_parentage_uses_git_graph_semantics_in_both_backends() {
    for mode in ["replacement", "packed_replacement", "graft"] {
        let repo = fixture();
        if mode == "graft" {
            std::fs::create_dir_all(repo.path().join(".git/info")).unwrap();
            std::fs::write(
                repo.path().join(".git/info/grafts"),
                format!("{}\n", repo.git(&["rev-parse", "topic"])),
            )
            .unwrap();
        } else {
            repo.git(&["replace", "--graft", "topic"]);
            if mode == "packed_replacement" {
                repo.git(&["pack-refs", "--all"]);
            }
        }
        let (expected, _) = output(&repo, "shell");
        let (native, debug) = output(&repo, "gix");
        assert_eq!(native, expected);
        assert!(native.contains("orphaned"));
        assert!(
            debug.contains(
                "backend_fallback requested=gix effective=shell reason=graph_interpretation"
            ),
            "{debug}"
        );
        assert!(debug.contains("cache=bypass"), "{debug}");
    }
}

#[cfg(unix)]
#[test]
fn invalid_argument_encoding_returns_a_cli_error_without_aborting() {
    use std::os::unix::ffi::OsStringExt;
    let repo = fixture();
    let invalid = std::ffi::OsString::from_vec(vec![255]);
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .arg(&invalid)
        .assert()
        .failure();
    assert_eq!(output.get_output().status.code(), Some(2));
    let diagnostic = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(!diagnostic.contains("panicked"));
    assert!(diagnostic.contains("UTF-8"), "{diagnostic}");
    // Invalid encoding at the private process boundary must also be harmless.
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .args(["--internal-cache-maintenance", "unused"])
        .arg(&invalid)
        .assert()
        .success();
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .arg("--internal-cache-maintenance")
        .arg(invalid)
        .arg("snapshot-00.json")
        .assert()
        .success();
}

#[test]
fn graph_environment_overrides_preserve_git_parentage() {
    for mode in [
        "replacement_namespace",
        "graft_file",
        "disabled_replacements",
    ] {
        let repo = fixture();
        let topic = repo.git(&["rev-parse", "topic"]);
        let (key, value) = if mode == "graft_file" {
            let file = repo.path().join("alternative-grafts");
            std::fs::write(&file, format!("{topic}\n")).unwrap();
            ("GIT_GRAFT_FILE", file.into_os_string())
        } else {
            repo.git(&["replace", "--graft", "topic"]);
            if mode == "replacement_namespace" {
                let replacement = repo.git(&["rev-parse", &format!("refs/replace/{topic}")]);
                repo.git(&["replace", "--delete", "topic"]);
                repo.git(&[
                    "update-ref",
                    &format!("refs/review-replacements/{topic}"),
                    &replacement,
                ]);
                ("GIT_REPLACE_REF_BASE", "refs/review-replacements/".into())
            } else {
                ("GIT_NO_REPLACE_OBJECTS", "1".into())
            }
        };
        let execute = |backend: &str| {
            let result = repo
                .command(assert_cmd::cargo::cargo_bin!("git-ls"))
                .env(key, &value)
                .args(["--backend", backend, "--debug", "--color", "never"])
                .assert()
                .success();
            (
                String::from_utf8(result.get_output().stdout.clone()).unwrap(),
                String::from_utf8(result.get_output().stderr.clone()).unwrap(),
            )
        };
        let (expected, _) = execute("shell");
        let (actual, debug) = execute("gix");
        assert_eq!(actual, expected);
        assert_eq!(actual.contains("orphaned"), mode != "disabled_replacements");
        assert!(debug.contains("effective=shell"), "{debug}");
        assert!(debug.contains("cache=bypass"));
    }
}

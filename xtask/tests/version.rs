use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[path = "../../tests/support/mod.rs"]
mod support;

use support::TestRepo;

const ZERO_OID: &str = "0000000000000000000000000000000000000000";

fn write_versions(repo: &TestRepo, manifest: &str, lock: &str) {
    fs::write(
        repo.path().join("Cargo.toml"),
        format!("[package]\nname = \"git-ls\"\nversion = \"{manifest}\"\nedition = \"2024\"\n"),
    )
    .unwrap();
    fs::write(
        repo.path().join("Cargo.lock"),
        format!("version = 4\n\n[[package]]\nname = \"git-ls\"\nversion = \"{lock}\"\n"),
    )
    .unwrap();
}

fn repository() -> TestRepo {
    let repo = TestRepo::new();
    fs::create_dir(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "").unwrap();
    write_versions(&repo, "1.2.3", "1.2.3");
    repo.commit("chore: initialise repository");
    repo.git(&["tag", "v1.2.3"]);
    repo
}

fn xtask(repo: &TestRepo) -> Command {
    let mut command = repo.command(assert_cmd::cargo::cargo_bin!("xtask"));
    command.arg("version");
    command
}

#[test]
fn commit_hook_generates_an_unstaged_bump_and_accepts_it_after_staging() {
    let repo = repository();
    let message = repo.path().join(".git/COMMIT_EDITMSG");
    fs::write(&message, "fix: handle empty repositories\n").unwrap();

    xtask(&repo)
        .arg("commit-msg")
        .arg(&message)
        .assert()
        .failure()
        .stderr(predicate::str::contains("1.2.3 -> 1.2.4"));
    for file in ["Cargo.toml", "Cargo.lock"] {
        assert!(
            fs::read_to_string(repo.path().join(file))
                .unwrap()
                .contains("1.2.4")
        );
        assert!(repo.git(&["show", &format!(":{file}")]).contains("1.2.3"));
    }

    repo.git(&["add", "Cargo.toml", "Cargo.lock"]);
    xtask(&repo)
        .arg("commit-msg")
        .arg(message)
        .assert()
        .success();
}

#[test]
fn commit_hook_rejects_an_incorrect_staged_bump_without_rewriting_it() {
    let repo = repository();
    write_versions(&repo, "1.3.0", "1.3.0");
    repo.git(&["add", "Cargo.toml", "Cargo.lock"]);
    let message = repo.path().join(".git/COMMIT_EDITMSG");
    fs::write(&message, "fix: handle empty repositories\n").unwrap();

    xtask(&repo)
        .arg("commit-msg")
        .arg(message)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires 1.2.4 from 1.2.3"));
    assert!(repo.git(&["diff"]).is_empty());
}

#[test]
fn verifies_patch_minor_and_breaking_commits_from_an_explicit_or_discovered_base() {
    let repo = repository();
    for (version, subject) in [
        ("1.2.4", "fix: handle empty repositories"),
        ("1.3.0", "feat: add a new palette"),
        ("2.0.0", "refactor!: replace the output format"),
    ] {
        write_versions(&repo, version, version);
        repo.commit(subject);
    }
    for base in ["v1.2.3", ZERO_OID, "HEAD"] {
        xtask(&repo)
            .args(["verify-range", base, "HEAD"])
            .assert()
            .success();
    }
}

#[test]
fn rejects_incorrect_version_bumps_and_lockfile_mismatches() {
    for (manifest, lock, expected) in [
        ("1.3.0", "1.3.0", "expected 1.2.4 from parent 1.2.3"),
        ("1.2.4", "1.2.3", "Cargo.lock records 1.2.3"),
    ] {
        let repo = repository();
        write_versions(&repo, manifest, lock);
        repo.commit("fix: handle empty repositories");
        xtask(&repo)
            .args(["verify-range", "v1.2.3", "HEAD"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
    }
}

#[test]
fn pre_push_checks_new_branches_and_ignores_deletions() {
    let repo = repository();
    write_versions(&repo, "1.2.4", "1.2.4");
    let head = repo.commit("fix: handle empty repositories");
    xtask(&repo)
        .arg("pre-push")
        .write_stdin(format!(
            "refs/heads/main {head} refs/heads/main {ZERO_OID}\n"
        ))
        .assert()
        .success();
    xtask(&repo)
        .arg("pre-push")
        .write_stdin(format!("(delete) {ZERO_OID} refs/heads/main {head}\n"))
        .assert()
        .success();
    xtask(&repo)
        .arg("pre-push")
        .write_stdin("invalid input\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid pre-push input line"));
}

#[test]
fn pre_push_rejects_invalid_versions_on_existing_branches() {
    let repo = repository();
    let base = repo.git(&["rev-parse", "HEAD"]);
    let head = repo.commit("fix: forgot the version bump");
    xtask(&repo)
        .arg("pre-push")
        .write_stdin(format!("refs/heads/main {head} refs/heads/main {base}\n"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected 1.2.4"));
}

#[test]
fn retries_failed_tag_pushes_without_recreating_annotated_tags() {
    let repo = repository();
    write_versions(&repo, "1.2.4", "1.2.4");
    let head = repo.commit("fix: handle empty repositories");
    xtask(&repo)
        .args(["tag-range", "v1.2.3", "HEAD"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("origin"));
    let tag = repo.git(&["rev-parse", "v1.2.4"]);

    let remote = tempfile::tempdir().unwrap();
    repo.command("git")
        .args(["init", "--bare"])
        .arg(remote.path())
        .assert()
        .success();
    repo.git(&["remote", "add", "origin", remote.path().to_str().unwrap()]);
    for _ in 0..2 {
        xtask(&repo)
            .args(["tag-range", "v1.2.3", "HEAD"])
            .assert()
            .success();
    }
    assert_eq!(repo.git(&["cat-file", "-t", "v1.2.4"]), "tag");
    assert_eq!(repo.git(&["rev-parse", "v1.2.4"]), tag);
    assert_eq!(repo.git(&["rev-parse", "v1.2.4^{}"]), head);
    assert!(
        repo.git(&["ls-remote", "origin", "refs/tags/v1.2.4^{}"])
            .contains(&head)
    );
}

#[test]
fn refuses_to_replace_a_conflicting_release_tag() {
    let repo = repository();
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["tag", "v1.2.4"]);
    write_versions(&repo, "1.2.4", "1.2.4");
    repo.commit("fix: handle empty repositories");

    xtask(&repo)
        .args(["tag-range", "v1.2.3", "HEAD"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tag v1.2.4 already points at"));
    assert_eq!(repo.git(&["rev-parse", "v1.2.4"]), base);
}

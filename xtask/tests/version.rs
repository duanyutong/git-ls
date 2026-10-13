#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

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
    assert_eq!(repo.git(&["diff"]), "");
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

#[test]
fn accepts_the_initial_version_and_checks_its_lockfile() {
    let repo = TestRepo::new();
    write_versions(&repo, "0.1.0", "0.1.0");
    repo.git(&["add", "."]);
    let message = repo.path().join(".git/COMMIT_EDITMSG");
    fs::write(&message, "feat: initialise repository\n").unwrap();
    xtask(&repo)
        .arg("commit-msg")
        .arg(&message)
        .assert()
        .success();
    repo.commit("feat: initialise repository");
    xtask(&repo)
        .args(["verify-range", ZERO_OID, "HEAD"])
        .assert()
        .success();
    write_versions(&repo, "0.1.0", "0.0.0");
    repo.git(&["add", "."]);
    xtask(&repo)
        .arg("commit-msg")
        .arg(&message)
        .assert()
        .failure()
        .stderr(predicate::str::contains("versions differ"));
}

#[test]
fn preserves_unstaged_manifest_changes() {
    let repo = repository();
    let manifest = repo.path().join("Cargo.toml");
    let original = fs::read_to_string(&manifest).unwrap() + "description = \"uncommitted\"\n";
    fs::write(&manifest, &original).unwrap();
    let message = repo.path().join(".git/COMMIT_EDITMSG");
    fs::write(&message, "fix: change behaviour\n").unwrap();
    xtask(&repo)
        .arg("commit-msg")
        .arg(&message)
        .assert()
        .failure()
        .stderr(predicate::str::contains("stage or restore"));
    assert_eq!(fs::read_to_string(manifest).unwrap(), original);
}

#[test]
fn reads_prek_revision_environment_instead_of_empty_standard_input() {
    let repo = repository();
    let base = repo.git(&["rev-parse", "HEAD"]);
    let head = repo.commit("fix: missing version bump");
    xtask(&repo)
        .arg("pre-push")
        .env("PRE_COMMIT_FROM_REF", base)
        .env("PRE_COMMIT_TO_REF", &head)
        .assert()
        .failure();
    xtask(&repo)
        .arg("pre-push")
        .env("PRE_COMMIT_TO_REF", head)
        .assert()
        .failure();
}

#[cfg(unix)]
fn install_commit_hook(repo: &TestRepo) {
    use std::os::unix::fs::PermissionsExt;
    let hooks = repo.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("commit-msg");
    let binary = assert_cmd::cargo::cargo_bin!("xtask")
        .to_str()
        .unwrap()
        .replace('\'', "'\\''");
    fs::write(
        &hook,
        format!("#!/bin/sh\nexec '{binary}' version commit-msg \"$1\"\n"),
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    repo.git(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

#[test]
#[cfg(unix)]
fn amends_root_and_later_commits_without_a_second_version_increment() {
    let repo = repository();
    install_commit_hook(&repo);
    xtask(&repo).args(["amend", "--no-edit"]).assert().success();
    assert!(repo.git(&["show", "HEAD:Cargo.toml"]).contains("1.2.3"));
    write_versions(&repo, "1.2.4", "1.2.4");
    repo.commit("fix: adjust behaviour");
    xtask(&repo).args(["amend", "--no-edit"]).assert().success();
    xtask(&repo)
        .args(["verify-range", "HEAD^", "HEAD"])
        .assert()
        .success();
    xtask(&repo)
        .args(["amend", "-m", "feat: requires a minor bump"])
        .assert()
        .failure();
}

#[test]
fn validates_the_entire_release_range_before_creating_any_tag() {
    let repo = repository();
    write_versions(&repo, "1.2.4", "1.2.4");
    repo.commit("fix: valid bump");
    repo.commit("fix: missing bump");
    xtask(&repo)
        .args(["tag-range", "v1.2.3", "HEAD"])
        .assert()
        .failure();
    assert_eq!(repo.git(&["tag", "--list"]), "v1.2.3");
}

#[test]
fn rejects_merge_commits() {
    let repo = repository();
    repo.git(&["checkout", "-b", "side"]);
    fs::write(repo.path().join("side"), "side").unwrap();
    repo.commit("fix: side change");
    repo.git(&["checkout", "main"]);
    fs::write(repo.path().join("main"), "main").unwrap();
    repo.commit("fix: main change");
    repo.git(&["merge", "--no-ff", "side", "-m", "fix: merge change"]);
    xtask(&repo)
        .args(["verify-range", "HEAD^", "HEAD"])
        .assert()
        .failure();
    xtask(&repo).args(["amend", "--no-edit"]).assert().failure();
}

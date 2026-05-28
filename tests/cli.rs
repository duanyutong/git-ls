use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_exits_successfully_without_git() {
    let mut cmd = Command::cargo_bin("git-ls").expect("binary exists");

    cmd.arg("--help").assert().success().stderr("").stdout(
        predicate::str::contains("Usage: git ls")
            .and(predicate::str::contains("--hidden"))
            .and(predicate::str::contains("--backend <VALUE>"))
            .and(predicate::str::contains("--order <VALUE>"))
            .and(predicate::str::contains("--color <VALUE>")),
    );
}

#[test]
fn invalid_option_exits_unsuccessfully_without_git() {
    let mut cmd = Command::cargo_bin("git-ls").expect("binary exists");

    cmd.arg("--order=later")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("invalid value 'later'"));
}

#[test]
fn version_includes_build_provenance() {
    let mut cmd = Command::cargo_bin("git-ls").expect("binary exists");

    cmd.arg("--version").assert().success().stderr("").stdout(
        predicate::str::contains(concat!("git ls ", env!("CARGO_PKG_VERSION"), " (git "))
            .and(predicate::str::contains("dirty="))
            .and(predicate::str::contains("target="))
            .and(predicate::str::contains("rustc="))
            .and(predicate::str::contains("built=")),
    );
}

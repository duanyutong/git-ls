use assert_cmd::Command;
use predicates::prelude::*;

mod support;

use support::TestRepo;

#[test]
fn help_exits_successfully_without_git() {
    let mut cmd = Command::cargo_bin("git-ls").expect("binary exists");

    cmd.arg("--help").assert().success().stderr("").stdout(
        predicate::str::contains("Usage: git ls")
            .and(predicate::str::contains("--hidden"))
            .and(predicate::str::contains("--backend <VALUE>"))
            .and(predicate::str::contains("--order <VALUE>"))
            .and(predicate::str::contains("--color <VALUE>"))
            .and(predicate::str::contains("-p, --palette <VALUE>")),
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

#[allow(clippy::expect_used)]
fn render(repo: &TestRepo, backend: &str, layout: &str) -> String {
    let output = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .args([
            "--backend",
            backend,
            "--layout",
            layout,
            "--color",
            "never",
            "--verbosity",
            "2",
        ])
        .assert()
        .success()
        .stderr("");
    String::from_utf8(output.get_output().stdout.clone()).expect("UTF-8 output")
}

#[test]
fn renders_real_stacks_identically_with_both_backends() {
    let repo = TestRepo::new();
    repo.commit("chore: initialise repository");
    repo.git(&["branch", "feature/merged"]);
    repo.git(&["checkout", "-b", "feature/base"]);
    repo.commit("feat: introduce workspace");
    repo.commit("fix: validate workspace");
    repo.git(&["checkout", "-b", "feature/child"]);
    let head = repo.commit("test: cover workspace");
    repo.git(&["checkout", "-b", "feature/independent", "main"]);
    repo.commit("ci: check dependencies");
    repo.git(&["checkout", "feature/child"]);

    for layout in ["inline", "columns"] {
        let output = render(&repo, "gix", layout);
        assert_eq!(output, render(&repo, "shell", layout));
        for (branch, count) in [
            ("feature/base", 2),
            ("feature/child", 1),
            ("feature/independent", 1),
        ] {
            let row = output
                .lines()
                .find(|line| line.contains(branch))
                .expect("branch row");
            let expected = if layout == "columns" {
                format!("{count} {branch}")
            } else {
                format!("({count}, ")
            };
            assert!(row.contains(&expected), "{row}");
        }
        assert!(output.contains("test: cover workspace"), "{output}");
        assert!(output.contains(&head[..7]), "{output}");
        assert!(output.contains('●') && output.contains('▶'), "{output}");
        assert!(!output.contains("feature/merged"), "{output}");
        assert!(!output.contains('\x1b'), "{output}");
    }
}

#[test]
fn renders_unrelated_history_as_an_orphan_with_both_backends() {
    let repo = TestRepo::new();
    repo.commit("chore: initialise repository");
    repo.git(&["checkout", "--orphan", "feature/island"]);
    repo.commit("feat: create independent history");

    let output = render(&repo, "gix", "columns");
    assert_eq!(output, render(&repo, "shell", "columns"));
    assert!(output.contains("feature/island"), "{output}");
    assert!(output.contains("(orphaned)"), "{output}");
    assert!(output.contains('⦸'), "{output}");
}

#[test]
fn renders_detached_head_with_both_backends() {
    let repo = TestRepo::new();
    repo.commit("chore: initialise repository");
    repo.git(&["checkout", "-b", "feature/topic"]);
    repo.commit("feat: add topic");
    repo.git(&["checkout", "--detach"]);

    let output = render(&repo, "gix", "columns");
    assert_eq!(output, render(&repo, "shell", "columns"));
    assert!(output.contains("feature/topic"), "{output}");
    assert!(output.contains('◉'), "{output}");
    assert!(!output.contains('▶'), "{output}");
}

#[test]
fn honours_repository_configuration_and_command_line_overrides() {
    let repo = TestRepo::new();
    repo.commit("chore: initialise repository");
    repo.git(&["branch", "-m", "trunk"]);
    repo.git(&["config", "branchless.core.mainBranch", "trunk"]);
    repo.git(&["config", "git-ls.backend", "shell"]);
    repo.git(&["config", "git-ls.verbosity", "0"]);
    repo.git(&["checkout", "-b", "feature/topic"]);
    repo.commit("feat: configure workspace");

    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::str::contains("feat: configure workspace").not());
    for backend in ["gix", "shell"] {
        let output = render(&repo, backend, "columns");
        assert!(output.contains("trunk"), "{output}");
        assert!(output.contains("feat: configure workspace"), "{output}");
    }
}

#[test]
fn reports_invalid_repository_configuration() {
    let repo = TestRepo::new();
    repo.git(&["config", "git-ls.verbosity", "9"]);
    repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "invalid git config git-ls.verbosity",
        ));
}

#[test]
fn reports_missing_main_branch_with_both_backends() {
    let repo = TestRepo::new();
    for backend in ["gix", "shell"] {
        repo.command(assert_cmd::cargo::cargo_bin!("git-ls"))
            .args(["--backend", backend])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicate::str::contains(
                "could not resolve plain Git main branch",
            ));
    }
}

#![allow(clippy::expect_used)]

mod support;

use support::TestRepo;

#[test]
fn diagnostics_preserve_output_and_report_startup_and_graph_work() {
    let repo = TestRepo::new();
    repo.commit("main");
    repo.git(&["checkout", "-b", "topic"]);
    repo.commit("topic");
    let binary = assert_cmd::cargo::cargo_bin!("git-ls");
    let ordinary = repo
        .command(binary)
        .args(["--color", "never"])
        .assert()
        .success();
    assert_eq!(ordinary.get_output().stderr, Vec::<u8>::new());
    let debug = repo
        .command(binary)
        .args(["--debug", "--color", "never"])
        .assert()
        .success();
    assert_eq!(ordinary.get_output().stdout, debug.get_output().stdout);
    let diagnostics = String::from_utf8_lossy(&debug.get_output().stderr);
    for field in [
        "stage=config",
        "stage=backend_open",
        "stage=stack_heads",
        "stage=lane_path",
        "stage=groups",
        "stage=layout",
        "stage=write_output",
        "parent=",
        "cumulative_ms=",
        "inclusive=true",
        "selected_tips=1",
        "status=ok observed_total_ms=",
    ] {
        assert!(
            diagnostics.contains(field),
            "missing {field}: {diagnostics}"
        );
    }
    for field in diagnostics.split_whitespace() {
        if let Some((name, value)) = field.split_once('=')
            && name.ends_with("_ms")
        {
            assert!(value.parse::<f64>().expect("numeric milliseconds") >= 0.0);
        }
    }
}

#[test]
fn configuration_failure_has_a_partial_performance_report() {
    let repo = TestRepo::new();
    repo.git(&["config", "git-ls.verbosity", "invalid"]);
    let result = repo
        .command(assert_cmd::cargo::cargo_bin!("git-ls"))
        .arg("--debug")
        .assert()
        .failure();
    let diagnostics = String::from_utf8_lossy(&result.get_output().stderr);
    assert!(diagnostics.contains("stage=config"));
    assert!(diagnostics.contains("status=error observed_total_ms="));
    assert!(diagnostics.contains("invalid git config"));
    assert_eq!(result.get_output().stdout, Vec::<u8>::new());
}

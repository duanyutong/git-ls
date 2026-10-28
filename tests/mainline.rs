#![allow(clippy::expect_used)]
mod support;

use support::TestRepo;

fn track(repo: &TestRepo, remote: &str, destination: &str, oid: &str) {
    repo.git(&["remote", "add", remote, "./unavailable-remote"]);
    repo.git(&["update-ref", destination, oid]);
    let upstream = destination
        .strip_prefix("refs/remotes/")
        .unwrap_or(destination);
    repo.git(&["branch", "--set-upstream-to", upstream, "main"]);
}

fn render(repo: &TestRepo, backend: &str, layout: &str, options: &[&str]) -> (String, String) {
    let mut command = repo.command(assert_cmd::cargo::cargo_bin!("git-ls"));
    command.args([
        "--backend",
        backend,
        "--layout",
        layout,
        "--color",
        "never",
        "--debug",
        "--no-cache-maintenance",
    ]);
    if !options.contains(&"--verbosity") {
        command.args(["--verbosity", "2"]);
    }
    let result = command.args(options).assert().success();
    (
        String::from_utf8(result.get_output().stdout.clone()).expect("UTF-8 output"),
        String::from_utf8(result.get_output().stderr.clone()).expect("UTF-8 diagnostics"),
    )
}

fn row<'a>(output: &'a str, text: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.contains(text))
        .expect("reference or branch row")
}

fn assert_count(output: &str, layout: &str, branch: &str, count: usize) {
    let line = row(output, branch);
    let expected = if layout == "columns" {
        format!("{count} {branch}")
    } else {
        format!("({count}, ")
    };
    assert!(line.contains(&expected), "{output}");
}

#[test]
fn cached_upstream_ahead_corrects_counts_and_diff_stats_and_preserves_the_gap() {
    let repo = TestRepo::new();
    let local = repo.commit("root");
    repo.git(&["checkout", "--detach"]);
    std::fs::write(repo.path().join("upstream.txt"), "one\ntwo\nthree\n").expect("upstream file");
    repo.commit("first upstream commit");
    let upstream = repo.commit("second upstream commit");
    track(&repo, "origin", "refs/remotes/origin/main", &upstream);
    repo.git(&["checkout", "-b", "feature/topic"]);
    std::fs::write(repo.path().join("feature.txt"), "one\ntwo\n").expect("feature file");
    repo.commit("feature change");
    for backend in ["gix", "shell"] {
        for layout in ["inline", "columns"] {
            let (output, _) = render(&repo, backend, layout, &[]);
            assert_count(&output, layout, "feature/topic", 1);
            let feature = row(&output, "feature/topic");
            assert!(feature.contains("+2") && feature.contains("-0"), "{output}");
            let remote_position = output
                .find("origin/main (cached)")
                .expect("cached upstream");
            let gap_position = output
                .find("(2 commits on origin/main)")
                .expect("upstream gap");
            let local_position = output.find(&local[..7]).expect("local main");
            assert!(
                remote_position < gap_position && gap_position < local_position,
                "{output}"
            );
            let (cached, debug) = render(&repo, backend, layout, &[]);
            assert!(debug.contains("cache=hit"), "{debug}");
            assert_eq!(cached, output);
            assert_eq!(render(&repo, backend, layout, &["--no-cache"]).0, output);
        }
    }
}

#[test]
fn mainline_endpoints_and_gaps_remain_visible_without_feature_branches() {
    for ahead in ["local", "remote"] {
        let repo = TestRepo::new();
        let root = repo.commit("root");
        if ahead == "remote" {
            repo.git(&["checkout", "--detach"]);
        }
        repo.commit("first main update");
        let tip = repo.commit("second main update");
        let remote = if ahead == "remote" { &tip } else { &root };
        track(&repo, "origin", "refs/remotes/origin/main", remote);
        for backend in ["gix", "shell"] {
            for layout in ["inline", "columns"] {
                let (output, _) = render(&repo, backend, layout, &[]);
                let expected = if ahead == "remote" {
                    "(2 commits on origin/main)"
                } else {
                    "(2 commits on main)"
                };
                assert!(output.contains(expected), "{output}");
                assert!(output.contains("origin/main (cached)"), "{output}");
                assert!(
                    output.contains(&root[..7]) && output.contains(&tip[..7]),
                    "{output}"
                );
                assert_eq!(render(&repo, backend, layout, &["--no-cache"]).0, output);
                let (minimal, _) = render(&repo, backend, layout, &["--verbosity", "0"]);
                assert!(minimal.contains(expected), "{minimal}");
                assert!(minimal.contains("origin/main (cached)"), "{minimal}");
            }
        }
    }
}

#[test]
fn equal_tips_coalesce_and_only_local_main_can_be_current() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    track(&repo, "upstream", "refs/remotes/upstream/main", &root);
    for backend in ["gix", "shell"] {
        for layout in ["inline", "columns"] {
            let (output, _) = render(&repo, backend, layout, &[]);
            assert!(
                row(&output, "main, upstream/main (cached)").contains('◆'),
                "{output}"
            );
            assert_eq!(output.matches(&root[..7]).count(), 1, "{output}");
            repo.git(&["checkout", "--detach"]);
            let (output, _) = render(&repo, backend, layout, &[]);
            assert!(!output.contains('◆') && !output.contains('▶'), "{output}");
            repo.git(&["checkout", "main"]);
        }
    }
}

#[test]
fn divergent_mainlines_keep_both_paths_and_feature_counts() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let local = repo.commit("local main change");
    repo.git(&["checkout", "-b", "feature/local"]);
    repo.commit("local feature");
    repo.git(&["checkout", "--detach", &root]);
    let upstream = repo.commit("upstream main change");
    track(&repo, "origin", "refs/remotes/origin/main", &upstream);
    repo.git(&["checkout", "-b", "feature/remote"]);
    repo.commit("remote feature");
    for backend in ["gix", "shell"] {
        for layout in ["inline", "columns"] {
            let (output, _) = render(&repo, backend, layout, &[]);
            assert_count(&output, layout, "feature/local", 1);
            assert_count(&output, layout, "feature/remote", 1);
            assert!(row(&output, &local[..7]).contains('◇'), "{output}");
            assert!(
                output.contains("origin/main (cached)") && output.contains(&root[..7]),
                "{output}"
            );
            assert_eq!(render(&repo, backend, layout, &["--no-cache"]).0, output);
            repo.git(&["checkout", "main"]);
            let (current, _) = render(&repo, backend, layout, &[]);
            assert!(row(&current, &local[..7]).contains('◆'), "{current}");
            assert!(
                !row(&current, "origin/main (cached)").contains('◆'),
                "{current}"
            );
            repo.git(&["checkout", "feature/remote"]);
        }
    }
}

#[test]
fn moving_or_removing_the_upstream_invalidates_topology() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    track(&repo, "origin", "refs/remotes/origin/main", &root);
    repo.git(&["checkout", "-b", "feature/topic"]);
    let first = repo.commit("first topic commit");
    repo.commit("second topic commit");
    for backend in ["gix", "shell"] {
        repo.git(&["update-ref", "refs/remotes/origin/main", &root]);
        let (before, _) = render(&repo, backend, "columns", &[]);
        assert_count(&before, "columns", "feature/topic", 2);
        assert!(
            render(&repo, backend, "columns", &[])
                .1
                .contains("cache=hit")
        );
        repo.git(&["update-ref", "refs/remotes/origin/main", &first]);
        let (after, debug) = render(&repo, backend, "columns", &[]);
        assert!(!debug.contains("cache=hit"), "{debug}");
        assert_count(&after, "columns", "feature/topic", 1);
        assert!(after.contains("(1 commit on origin/main)"), "{after}");
        repo.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
        let (missing, debug) = render(&repo, backend, "columns", &[]);
        assert!(!debug.contains("cache=hit"), "{debug}");
        assert!(!missing.contains("(cached)"), "{missing}");
        assert_count(&missing, "columns", "feature/topic", 2);
    }
}

#[test]
fn divergent_mainline_fork_points_bound_feature_counts_and_tree_comparisons() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    std::fs::write(repo.path().join("local-main.txt"), "main content\n").expect("main file");
    let fork = repo.commit("first local main update");
    repo.commit("second local main update");
    repo.git(&["checkout", "-b", "feature/older-local", &fork]);
    std::fs::write(repo.path().join("feature.txt"), "feature content\n").expect("feature file");
    repo.commit("feature change");
    repo.git(&["checkout", "--detach", &root]);
    let upstream = repo.commit("remote main update");
    track(&repo, "origin", "refs/remotes/origin/main", &upstream);
    repo.git(&["checkout", "feature/older-local"]);
    for backend in ["gix", "shell"] {
        for layout in ["inline", "columns"] {
            let (output, _) = render(&repo, backend, layout, &[]);
            assert_count(&output, layout, "feature/older-local", 1);
            assert!(
                row(&output, "feature/older-local").contains("+1"),
                "{output}"
            );
            assert!(row(&output, &fork[..7]).contains('◇'), "{output}");
            assert_eq!(render(&repo, backend, layout, &["--no-cache"]).0, output);
            let (minimal, _) = render(&repo, backend, layout, &["--verbosity", "0"]);
            assert!(minimal.contains("first local main update"), "{minimal}");
            assert_eq!(
                render(&repo, backend, layout, &["--no-cache", "--verbosity", "0"]).0,
                minimal
            );
        }
    }
}

#[test]
fn explicit_upstream_and_custom_fetch_refspec_take_precedence_over_other_remote_names() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    repo.git(&["update-ref", "refs/remotes/origin/main", &root]);
    repo.git(&["checkout", "--detach"]);
    let upstream = repo.commit("upstream change");
    repo.git(&["remote", "add", "company", "./unavailable-remote"]);
    repo.git(&[
        "config",
        "remote.company.fetch",
        "+refs/heads/trunk:refs/cache/trunk",
    ]);
    repo.git(&["config", "branch.main.remote", "company"]);
    repo.git(&["config", "branch.main.merge", "refs/heads/trunk"]);
    repo.git(&["update-ref", "refs/cache/trunk", &upstream]);
    repo.git(&["checkout", "-b", "feature/topic"]);
    repo.commit("feature change");
    for backend in ["gix", "shell"] {
        let (output, _) = render(&repo, backend, "columns", &[]);
        assert_count(&output, "columns", "feature/topic", 1);
        assert!(output.contains("refs/cache/trunk (cached)"), "{output}");
        assert!(!output.contains("origin/main"), "{output}");
        repo.git(&["config", "--unset", "branch.main.remote"]);
        let (unconfigured, _) = render(&repo, backend, "columns", &[]);
        assert_count(&unconfigured, "columns", "feature/topic", 2);
        assert!(!unconfigured.contains("(cached)"), "{unconfigured}");
        repo.git(&["config", "branch.main.remote", "company"]);
    }
}

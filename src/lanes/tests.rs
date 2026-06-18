use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use crate::backend::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, RepositoryStateBackend,
};
use crate::cli::{Backend, ColourMode, Layout, Order, Palette, RuntimeOptions, Verbosity};
use crate::model::{
    BranchAnnotation, BranchPoint, BranchPointRef, BuiltLanes, CommitMeta, Lane, RewrittenCommit,
};

#[derive(Default)]
struct FakeLaneBackend {
    revsets: HashMap<(String, bool), Vec<String>>,
    branch_names: HashMap<(String, bool), Vec<String>>,
    local_branches: HashMap<String, Vec<String>>,
    current_head: Option<String>,
    current_branch: Option<String>,
    main_name: String,
    local_branches_error: Option<String>,
    current_state_error: Option<String>,
    main_name_error: Option<String>,
    merge_bases: HashMap<(String, String), Option<String>>,
    ancestry_paths: HashMap<(Option<String>, String), Vec<String>>,
    metas: HashMap<String, CommitMeta>,
    metadata_requests: RefCell<Vec<Vec<String>>>,
}

impl FakeLaneBackend {
    fn with_revset(mut self, revset: &str, hidden: bool, oids: &[&str]) -> Self {
        self.revsets.insert(
            (revset.to_string(), hidden),
            oids.iter().map(|oid| (*oid).to_string()).collect(),
        );
        self
    }

    fn with_branch_names(mut self, revset: &str, hidden: bool, names: &[&str]) -> Self {
        self.branch_names.insert(
            (revset.to_string(), hidden),
            names.iter().map(|name| (*name).to_string()).collect(),
        );
        self
    }

    fn with_local_branches(mut self, branches: &[(&str, &[&str])]) -> Self {
        self.local_branches = branches
            .iter()
            .map(|(oid, names)| {
                (
                    (*oid).to_string(),
                    names.iter().map(|name| (*name).to_string()).collect(),
                )
            })
            .collect();
        self
    }

    fn with_head(mut self, head: Option<&str>, branch: Option<&str>) -> Self {
        self.current_head = head.map(ToOwned::to_owned);
        self.current_branch = branch.map(ToOwned::to_owned);
        self
    }

    fn with_main_name(mut self, main_name: &str) -> Self {
        self.main_name = main_name.to_string();
        self
    }

    fn with_local_branches_error(mut self, message: &str) -> Self {
        self.local_branches_error = Some(message.to_string());
        self
    }

    fn with_current_state_error(mut self, message: &str) -> Self {
        self.current_state_error = Some(message.to_string());
        self
    }

    fn with_main_name_error(mut self, message: &str) -> Self {
        self.main_name_error = Some(message.to_string());
        self
    }

    fn with_merge_base(mut self, main_oid: &str, head_oid: &str, base_oid: Option<&str>) -> Self {
        self.merge_bases.insert(
            (main_oid.to_string(), head_oid.to_string()),
            base_oid.map(ToOwned::to_owned),
        );
        self
    }

    fn with_ancestry_path(mut self, base_oid: Option<&str>, head_oid: &str, path: &[&str]) -> Self {
        self.ancestry_paths.insert(
            (base_oid.map(ToOwned::to_owned), head_oid.to_string()),
            path.iter().map(|oid| (*oid).to_string()).collect(),
        );
        self
    }

    fn with_meta(mut self, oid: &str, subject: &str, timestamp: i64) -> Self {
        self.metas
            .insert(oid.to_string(), CommitMeta::new(oid, timestamp, subject));
        self
    }

    fn metadata_requests(&self) -> Vec<Vec<String>> {
        self.metadata_requests.borrow().clone()
    }
}

fn missing_fixture(name: &str, key: impl std::fmt::Debug) -> GitLsError {
    GitLsError::TestFixture(format!("missing {name} fixture for {key:?}"))
}

fn insert_fake_meta(cache: &mut HashMap<String, CommitMeta>, alias: &str, meta: CommitMeta) {
    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
}

impl BranchlessQueries for FakeLaneBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.revsets
            .get(&(revset.to_string(), hidden))
            .cloned()
            .ok_or_else(|| missing_fixture("revset", (revset, hidden)))
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.branch_names
            .get(&(revset.to_string(), hidden))
            .cloned()
            .ok_or_else(|| missing_fixture("branch names", (revset, hidden)))
    }
}

impl RepositoryStateBackend for FakeLaneBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        if let Some(message) = self.local_branches_error.as_ref() {
            return Err(GitLsError::TestFixture(message.clone()));
        }
        Ok(self.local_branches.clone())
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        if let Some(message) = self.current_state_error.as_ref() {
            return Err(GitLsError::TestFixture(message.clone()));
        }
        Ok((self.current_head.clone(), self.current_branch.clone()))
    }

    fn main_branch_name(&self) -> Result<String> {
        if let Some(message) = self.main_name_error.as_ref() {
            return Err(GitLsError::TestFixture(message.clone()));
        }
        if self.main_name.is_empty() {
            Ok("main".to_string())
        } else {
            Ok(self.main_name.clone())
        }
    }
}

impl CommitMetadataBackend for FakeLaneBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        self.metadata_requests
            .borrow_mut()
            .push(oids.iter().map(|oid| (*oid).to_string()).collect());
        for oid in oids {
            let meta = self
                .metas
                .get(*oid)
                .cloned()
                .ok_or_else(|| missing_fixture("metadata", oid))?;
            insert_fake_meta(cache, oid, meta);
        }
        Ok(())
    }
}

impl AncestryBackend for FakeLaneBackend {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        self.merge_bases
            .get(&(main_oid.to_string(), head_oid.to_string()))
            .cloned()
            .ok_or_else(|| missing_fixture("merge base", (main_oid, head_oid)))
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        self.ancestry_paths
            .get(&(base_oid.map(ToOwned::to_owned), head_oid.to_string()))
            .cloned()
            .ok_or_else(|| missing_fixture("ancestry path", (base_oid, head_oid)))
    }
}

fn point(oid: &str, names: &[&str]) -> BranchPoint {
    BranchPoint::new(oid, names.iter().copied(), None)
}

fn point_ref(oid: &str, names: &[&str]) -> BranchPointRef {
    BranchPointRef::new(oid, names.iter().copied())
}

fn point_with_count_at(
    oid: &str,
    names: &[&str],
    commit_count: usize,
    subject: &str,
    timestamp: i64,
) -> BranchPoint {
    BranchPoint::new(
        oid,
        names.iter().copied(),
        Some(BranchAnnotation::new(
            CommitMeta::new(oid, timestamp, subject),
            commit_count,
        )),
    )
}

fn lane(oid: &str, base: Option<&str>, timestamp: i64, contains_current: bool) -> Lane {
    Lane::new(
        oid,
        base.map(ToOwned::to_owned),
        vec![point(oid, &[oid])],
        timestamp,
        contains_current,
    )
}

fn lane_group(base: Option<&str>, main_distance: Option<usize>, lanes: Vec<Lane>) -> LaneGroup {
    LaneGroup::new(base.map(ToOwned::to_owned), None, main_distance, lanes)
}

fn lane_context<'a>(
    current_branch: Option<&'a str>,
    head: Option<&'a str>,
    verbosity: Verbosity,
    detect_rewritten_commits: bool,
) -> LaneBuildContext<'a> {
    LaneBuildContext {
        current_branch,
        head,
        verbosity,
        detect_rewritten_commits,
        debug: false,
    }
}

fn runtime_options(verbosity: Verbosity) -> RuntimeOptions {
    RuntimeOptions {
        revset: "draft()".to_string(),
        hidden: false,
        debug: false,
        verbosity,
        backend: Backend::Gix,
        order: Order::Newest,
        colour_mode: ColourMode::Never,
        palette: Palette::Classic,
        layout: Layout::Inline,
    }
}

fn runtime_options_with_revset(revset: &str, verbosity: Verbosity) -> RuntimeOptions {
    let mut args = runtime_options(verbosity);
    args.revset = revset.to_string();
    args
}

fn assert_test_fixture_error(error: GitLsError, expected: &str) {
    let GitLsError::TestFixture(message) = error else {
        panic!("expected test fixture error");
    };
    assert!(
        message.contains(expected),
        "expected {message:?} to contain {expected:?}"
    );
}

#[test]
fn creates_branch_revset() {
    assert_eq!(
        branch_revset("draft()"),
        "((draft()) & branches()) - public()"
    );
}

#[test]
fn uses_plain_git_fallback_trims_default_revset() {
    assert!(uses_plain_git_fallback(" draft() "));
    assert!(!uses_plain_git_fallback("custom()"));
}

#[test]
fn maps_selected_branch_points_by_oid() {
    let mut branches = HashMap::new();
    branches.insert(
        "a".to_string(),
        vec!["zeta".to_string(), "alpha".to_string(), "main".to_string()],
    );
    branches.insert("b".to_string(), vec!["ignored".to_string()]);

    let selected = vec!["alpha".to_string(), "zeta".to_string()];
    let points = branch_points_by_oid(&selected, &branches);

    assert_eq!(points.len(), 1);
    assert_eq!(
        points.get("a").unwrap(),
        &point_ref("a", &["alpha", "zeta"])
    );
}

#[test]
fn rejects_ambiguous_main_revsets_before_lane_construction() {
    let git = FakeLaneBackend::default().with_revset("main()", false, &["one", "two"]);
    let args = runtime_options(Verbosity::Low);
    let mut cache = HashMap::new();

    let error = build_lanes(&git, &args, &mut cache).unwrap_err();

    assert!(matches!(
        error,
        GitLsError::AmbiguousMainRevset { count } if count == 2
    ));
}

#[test]
fn propagates_lane_selection_query_and_repository_errors() {
    let revset = "((custom()) & branches()) - public()";
    let args = runtime_options_with_revset("custom()", Verbosity::Low);
    let cases = [
        (
            FakeLaneBackend::default().with_revset("main()", false, &["main-oid"]),
            "branch names",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &[])
                .with_current_state_error("current state failed"),
            "current state failed",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &[])
                .with_main_name_error("main branch failed"),
            "main branch failed",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_local_branches_error("local branches failed"),
            "local branches failed",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_local_branches(&[("head", &["feature/one"])]),
            "revset",
        ),
    ];

    for (git, expected) in cases {
        let mut cache = HashMap::new();

        let error = build_lanes(&git, &args, &mut cache).unwrap_err();

        assert_test_fixture_error(error, expected);
    }
}

#[test]
fn propagates_custom_branchless_main_query_errors() {
    let args = runtime_options_with_revset("custom()", Verbosity::Low);
    let mut cache = HashMap::new();

    let error = build_lanes(&FakeLaneBackend::default(), &args, &mut cache).unwrap_err();

    assert_test_fixture_error(error, "revset");
}

#[test]
fn default_branchless_selection_falls_back_when_branch_or_head_queries_fail() {
    let default_revset = branch_revset(DEFAULT_REVSET);
    let missing_branch_names =
        FakeLaneBackend::default().with_revset("main()", false, &["main-oid"]);

    assert!(
        query_branchless_lane_selection(&missing_branch_names, DEFAULT_REVSET, false, false)
            .unwrap()
            .is_none()
    );

    let missing_heads = FakeLaneBackend::default()
        .with_revset("main()", false, &["main-oid"])
        .with_branch_names(&default_revset, false, &["feature/one"])
        .with_head(Some("head"), Some("feature/one"))
        .with_main_name("main")
        .with_local_branches(&[("head", &["feature/one"])]);

    assert!(
        query_branchless_lane_selection(&missing_heads, DEFAULT_REVSET, false, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn query_branchless_lane_selection_builds_populated_selection() {
    let user_revset = "custom()";
    let revset = branch_revset(user_revset);
    let git = FakeLaneBackend::default()
        .with_revset("main()", true, &["main-oid"])
        .with_branch_names(&revset, true, &["feature/one", "feature/two"])
        .with_head(Some("tip"), Some("feature/two"))
        .with_main_name("trunk")
        .with_local_branches(&[
            ("base", &["feature/one"]),
            ("tip", &["feature/two"]),
            ("main-oid", &["trunk"]),
        ])
        .with_revset(&format!("heads({revset})"), true, &["tip"]);

    let Some(LaneSelection::Populated {
        main_oid,
        head_oids,
        points_by_oid,
        repository,
        detect_rewritten_commits,
    }) = query_branchless_lane_selection(&git, user_revset, true, false).unwrap()
    else {
        panic!("expected populated branchless selection");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(head_oids, vec!["tip".to_string()]);
    assert_eq!(repository.main_name, "trunk");
    assert_eq!(repository.current_branch.as_deref(), Some("feature/two"));
    assert_eq!(
        points_by_oid.get("base"),
        Some(&point_ref("base", &["feature/one"]))
    );
    assert_eq!(
        points_by_oid.get("tip"),
        Some(&point_ref("tip", &["feature/two"]))
    );
    assert!(detect_rewritten_commits);
}

#[test]
fn builds_empty_branchless_selection_for_custom_empty_revsets() {
    let revset = "((custom()) & branches()) - public()";
    let git = FakeLaneBackend::default()
        .with_revset("main()", false, &["main-oid"])
        .with_branch_names(revset, false, &[])
        .with_head(Some("main-oid"), Some("main"))
        .with_main_name("main");
    let args = runtime_options_with_revset("custom()", Verbosity::Low);
    let mut cache = HashMap::new();

    let BuiltLanes::Empty {
        main_oid,
        repository,
    } = build_lanes(&git, &args, &mut cache).unwrap()
    else {
        panic!("expected empty selection");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(repository.main_name, "main");
    assert_eq!(repository.head.as_deref(), Some("main-oid"));
}

#[test]
fn falls_back_to_plain_git_when_branchless_is_unavailable() {
    let git = FakeLaneBackend::default()
        .with_local_branches(&[
            ("main-oid", &["main"]),
            ("base", &["feature/base"]),
            ("tip", &["feature/tip"]),
        ])
        .with_head(Some("tip"), Some("feature/tip"))
        .with_merge_base("base", "main-oid", Some("main-oid"))
        .with_merge_base("tip", "main-oid", Some("main-oid"))
        .with_merge_base("base", "tip", Some("base"))
        .with_merge_base("tip", "base", Some("base"))
        .with_merge_base("main-oid", "tip", Some("main-oid"))
        .with_meta("tip", "tip", 20)
        .with_ancestry_path(Some("main-oid"), "tip", &["base", "tip"]);
    let mut cache = HashMap::new();

    let BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
    } = build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(repository.main_name, "main");
    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].head_oid, "tip");
    assert!(lanes[0].contains_current);
    assert_eq!(
        lanes[0].branch_points,
        vec![
            point("tip", &["feature/tip"]),
            point("base", &["feature/base"]),
        ]
    );
}

#[test]
fn populated_branchless_selection_propagates_repository_snapshot_errors() {
    let revset = "((custom()) & branches()) - public()";
    let args = runtime_options_with_revset("custom()", Verbosity::Low);
    let cases = [
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_current_state_error("current state failed"),
            "current state failed",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_head(Some("head"), Some("feature/one"))
                .with_main_name_error("main branch failed"),
            "main branch failed",
        ),
    ];

    for (git, expected) in cases {
        let mut cache = HashMap::new();

        let error = build_lanes(&git, &args, &mut cache).unwrap_err();

        assert_test_fixture_error(error, expected);
    }
}

#[test]
fn falls_back_to_plain_git_when_default_branchless_selection_is_empty() {
    let revset = "((draft()) & branches()) - public()";
    let git = FakeLaneBackend::default()
        .with_revset("main()", false, &["branchless-main"])
        .with_branch_names(revset, false, &[])
        .with_local_branches(&[("main-oid", &["main"]), ("tip", &["feature/tip"])])
        .with_head(Some("tip"), Some("feature/tip"))
        .with_merge_base("tip", "main-oid", Some("main-oid"))
        .with_merge_base("main-oid", "tip", Some("main-oid"))
        .with_meta("tip", "tip", 20)
        .with_ancestry_path(Some("main-oid"), "tip", &["tip"]);
    let mut cache = HashMap::new();

    let BuiltLanes::Populated {
        lanes, main_oid, ..
    } = build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].branch_points, vec![point("tip", &["feature/tip"])]);
}

#[test]
fn plain_git_fallback_excludes_branches_already_merged_to_main() {
    let git = FakeLaneBackend::default()
        .with_local_branches(&[("main-oid", &["main"]), ("old", &["feature/old"])])
        .with_head(Some("main-oid"), Some("main"))
        .with_merge_base("old", "main-oid", Some("old"));
    let mut cache = HashMap::new();

    let BuiltLanes::Empty { main_oid, .. } =
        build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap()
    else {
        panic!("expected empty selection");
    };

    assert_eq!(main_oid, "main-oid");
}

#[test]
fn plain_git_fallback_propagates_repository_state_errors() {
    let cases = [
        (
            FakeLaneBackend::default()
                .with_local_branches(&[("main-oid", &["main"])])
                .with_main_name_error("main branch failed"),
            "main branch failed",
        ),
        (
            FakeLaneBackend::default()
                .with_local_branches(&[("main-oid", &["main"])])
                .with_current_state_error("current state failed"),
            "current state failed",
        ),
    ];

    for (git, expected) in cases {
        let mut cache = HashMap::new();

        let error = build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap_err();

        assert_test_fixture_error(error, expected);
    }
}

#[test]
fn plain_git_fallback_propagates_main_resolution_and_ancestry_errors() {
    let missing_main = FakeLaneBackend::default().with_local_branches(&[("tip", &["feature/tip"])]);
    let mut cache = HashMap::new();
    assert!(matches!(
        build_lanes(&missing_main, &runtime_options(Verbosity::Low), &mut cache),
        Err(GitLsError::PlainGitMainBranchNotFound { .. })
    ));

    let missing_branch_ancestry = FakeLaneBackend::default()
        .with_local_branches(&[("main-oid", &["main"]), ("tip", &["feature/tip"])])
        .with_head(Some("tip"), Some("feature/tip"));
    let mut cache = HashMap::new();
    let error = build_lanes(
        &missing_branch_ancestry,
        &runtime_options(Verbosity::Low),
        &mut cache,
    )
    .unwrap_err();
    assert_test_fixture_error(error, "merge base");

    let missing_stack_ancestry = FakeLaneBackend::default()
        .with_local_branches(&[
            ("main-oid", &["main"]),
            ("first", &["feature/first"]),
            ("second", &["feature/second"]),
        ])
        .with_head(Some("first"), Some("feature/first"))
        .with_merge_base("first", "main-oid", None)
        .with_merge_base("second", "main-oid", None);
    let mut cache = HashMap::new();
    let error = build_lanes(
        &missing_stack_ancestry,
        &runtime_options(Verbosity::Low),
        &mut cache,
    )
    .unwrap_err();
    assert_test_fixture_error(error, "merge base");
}

#[test]
fn plain_git_main_branch_uses_fallback_candidates_and_reports_absence() {
    let branch_oid_map = HashMap::from([("trunk-oid".to_string(), vec!["trunk".to_string()])]);

    assert_eq!(
        plain_git_main_branch(&branch_oid_map, "missing").unwrap(),
        ("trunk".to_string(), "trunk-oid".to_string())
    );

    let error = plain_git_main_branch(&HashMap::new(), "missing").unwrap_err();
    assert!(matches!(
        error,
        GitLsError::PlainGitMainBranchNotFound { .. }
    ));
    assert_eq!(
        error.to_string(),
        "could not resolve plain Git main branch from local branches; tried missing, main, master, trunk"
    );
}

#[test]
fn plain_git_branch_names_sorts_deduplicates_and_excludes_main_alias() {
    let git = FakeLaneBackend::default()
        .with_merge_base("feature-a", "main-oid", Some("main-oid"))
        .with_merge_base("feature-b", "main-oid", Some("main-oid"));
    let branch_oid_map = HashMap::from([
        ("main-oid".to_string(), vec!["main".to_string()]),
        (
            "feature-a".to_string(),
            vec![
                "feature/z".to_string(),
                "main".to_string(),
                "feature/a".to_string(),
            ],
        ),
        (
            "feature-b".to_string(),
            vec!["feature/a".to_string(), "feature/b".to_string()],
        ),
    ]);

    let branch_names = plain_git_branch_names(&git, &branch_oid_map, "main", "main-oid").unwrap();

    assert_eq!(
        branch_names,
        vec![
            "feature/a".to_string(),
            "feature/b".to_string(),
            "feature/z".to_string(),
        ]
    );
}

#[test]
fn plain_git_stack_heads_handles_empty_and_identical_inputs() {
    let git = FakeLaneBackend::default();
    let empty: Vec<String> = Vec::new();
    assert_eq!(
        plain_git_stack_heads(&git, empty.iter()).unwrap(),
        Vec::<String>::new()
    );

    let selected = ["same".to_string()];
    assert_eq!(
        plain_git_stack_heads(&git, selected.iter()).unwrap(),
        vec!["same".to_string()]
    );
    assert!(is_ancestor(&git, "same", "same").unwrap());
}

#[test]
fn plain_git_stack_heads_keeps_independent_heads_and_drops_ancestors() {
    let git = FakeLaneBackend::default()
        .with_merge_base("base", "other", None)
        .with_merge_base("base", "tip", Some("base"))
        .with_merge_base("other", "base", None)
        .with_merge_base("other", "tip", None)
        .with_merge_base("tip", "base", Some("base"))
        .with_merge_base("tip", "other", None);
    let selected = ["base".to_string(), "tip".to_string(), "other".to_string()];

    let heads = plain_git_stack_heads(&git, selected.iter()).unwrap();

    assert_eq!(heads, vec!["other".to_string(), "tip".to_string()]);
}

#[test]
fn propagates_lane_build_metadata_and_ancestry_errors() {
    let revset = "((draft()) & branches()) - public()";
    let cases = [
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_local_branches(&[("head", &["feature/one"])])
                .with_revset(&format!("heads({revset})"), false, &["head"]),
            "metadata",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_local_branches(&[("head", &["feature/one"])])
                .with_revset(&format!("heads({revset})"), false, &["head"])
                .with_meta("head", "head", 10),
            "merge base",
        ),
        (
            FakeLaneBackend::default()
                .with_revset("main()", false, &["main-oid"])
                .with_branch_names(revset, false, &["feature/one"])
                .with_local_branches(&[("head", &["feature/one"])])
                .with_revset(&format!("heads({revset})"), false, &["head"])
                .with_meta("head", "head", 10)
                .with_merge_base("main-oid", "head", Some("main-oid")),
            "ancestry path",
        ),
    ];

    for (git, expected) in cases {
        let mut cache = HashMap::new();

        let error = build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap_err();

        assert_test_fixture_error(error, expected);
    }
}

#[test]
fn prefetch_lane_metadata_requests_branch_points_when_metadata_is_rendered() {
    let git = FakeLaneBackend::default()
        .with_meta("base", "base", 10)
        .with_meta("child", "child", 20)
        .with_meta("tip", "tip", 30);
    let head_oids = vec!["tip".to_string(), "child".to_string(), "tip".to_string()];
    let points_by_oid = HashMap::from([
        ("base".to_string(), point_ref("base", &["feature/base"])),
        ("child".to_string(), point_ref("child", &["feature/child"])),
    ]);
    let mut cache = HashMap::new();

    prefetch_lane_metadata(
        &git,
        &head_oids,
        &points_by_oid,
        Verbosity::Medium,
        &mut cache,
    )
    .unwrap();

    assert_eq!(
        git.metadata_requests(),
        vec![vec![
            "base".to_string(),
            "child".to_string(),
            "tip".to_string(),
        ]]
    );
}

#[test]
fn prefetch_lane_metadata_requests_only_heads_at_low_verbosity() {
    let git = FakeLaneBackend::default()
        .with_meta("child", "child", 20)
        .with_meta("tip", "tip", 30);
    let head_oids = vec!["tip".to_string(), "child".to_string(), "tip".to_string()];
    let points_by_oid = HashMap::from([
        ("base".to_string(), point_ref("base", &["feature/base"])),
        ("child".to_string(), point_ref("child", &["feature/child"])),
    ]);
    let mut cache = HashMap::new();

    prefetch_lane_metadata(&git, &head_oids, &points_by_oid, Verbosity::Low, &mut cache).unwrap();

    assert_eq!(
        git.metadata_requests(),
        vec![vec!["child".to_string(), "tip".to_string()]]
    );
}

#[test]
fn collect_lane_path_handles_orphaned_history() {
    let git = FakeLaneBackend::default()
        .with_merge_base("main-oid", "head", None)
        .with_ancestry_path(None, "head", &["root", "head"]);

    let lane_path = collect_lane_path(&git, "main-oid", "head").unwrap();

    assert_eq!(
        lane_path,
        LanePath {
            head_oid: "head".to_string(),
            base_oid: None,
            ancestry_path: vec!["root".to_string(), "head".to_string()],
        }
    );
}

#[test]
fn orders_lanes_by_time_and_oid_without_current_promotion() {
    let lanes = vec![
        lane("older", Some("main"), 10, false),
        lane("current", Some("main"), 15, true),
        lane("newer-b", Some("main"), 20, false),
        lane("newer-a", Some("main"), 20, false),
    ];

    let newest: Vec<String> = ordered_lanes(lanes.clone(), Order::Newest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(newest, vec!["newer-a", "newer-b", "current", "older"]);

    let oldest: Vec<String> = ordered_lanes(lanes, Order::Oldest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();
    assert_eq!(oldest, vec!["older", "current", "newer-a", "newer-b"]);
}

#[test]
fn orders_lanes_by_head_oid_when_timestamps_match() {
    let lanes = vec![
        lane("same-c", Some("main"), 20, false),
        lane("same-a", Some("main"), 20, false),
        lane("same-b", Some("main"), 20, false),
    ];

    let ordered: Vec<String> = ordered_lanes(lanes, Order::Newest)
        .into_iter()
        .map(|lane| lane.head_oid)
        .collect();

    assert_eq!(ordered, vec!["same-a", "same-b", "same-c"]);
}

#[test]
fn groups_lanes_by_base_time_without_current_promotion() {
    let lanes = vec![
        lane("a", Some("base-a"), 30, false),
        lane("b", Some("base-b"), 2, true),
        lane("c", Some("base-a"), 10, false),
    ];

    let groups = grouped_by_base(lanes.clone(), Order::Newest);

    assert_eq!(groups[0].0, Some("base-a".to_string()));
    assert_eq!(groups[0].1.len(), 2);
    assert_eq!(groups[1].0, Some("base-b".to_string()));
    assert_eq!(groups[1].1.len(), 1);

    let groups = grouped_by_base(lanes, Order::Oldest);

    assert_eq!(groups[0].0, Some("base-b".to_string()));
    assert_eq!(groups[0].1.len(), 1);
    assert_eq!(groups[1].0, Some("base-a".to_string()));
    assert_eq!(groups[1].1.len(), 2);
}

#[test]
fn groups_lanes_with_base_oid_tie_breaker_when_timestamps_match() {
    let lanes = vec![
        lane("b", Some("base-b"), 10, false),
        lane("a", Some("base-a"), 10, false),
    ];

    let groups = grouped_by_base(lanes, Order::Newest);

    assert_eq!(groups[0].0, Some("base-a".to_string()));
    assert_eq!(groups[1].0, Some("base-b".to_string()));
}

#[test]
fn lane_group_order_places_main_history_before_orphaned_groups() {
    let mut groups = [
        lane_group(None, None, vec![lane("orphan", None, 100, false)]),
        lane_group(
            Some("main"),
            Some(0),
            vec![lane("connected", Some("main"), 1, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("main".to_string()));
    assert_eq!(groups[1].base_oid, None);
}

#[test]
fn lane_group_order_places_orphaned_groups_after_main_history() {
    let mut groups = [
        lane_group(
            Some("main"),
            Some(0),
            vec![lane("connected", Some("main"), 1, false)],
        ),
        lane_group(None, None, vec![lane("orphan", None, 100, false)]),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("main".to_string()));
    assert_eq!(groups[1].base_oid, None);
}

#[test]
fn builds_lane_groups_with_main_history_distances() {
    let git = FakeLaneBackend::default()
        .with_meta("old-main", "old base", 1_700_000_001)
        .with_ancestry_path(
            Some("old-main"),
            "main-oid",
            &["main-1", "main-2", "main-oid"],
        );
    let lanes = vec![
        lane("current", Some("main-oid"), 2, false),
        lane("old", Some("old-main"), 1, true),
    ];
    let mut cache = HashMap::new();

    let groups =
        build_lane_groups(&git, lanes, "main-oid", Order::Newest, false, &mut cache).unwrap();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].base_oid, Some("main-oid".to_string()));
    assert_eq!(groups[0].main_distance, Some(0));
    assert_eq!(groups[0].base_meta, None);
    assert_eq!(groups[1].base_oid, Some("old-main".to_string()));
    assert_eq!(groups[1].main_distance, Some(3));
    assert_eq!(
        groups[1]
            .base_meta
            .as_ref()
            .map(|meta| meta.subject.as_str()),
        Some("old base")
    );
}

#[test]
fn builds_lane_groups_without_main_distance_for_empty_or_orphaned_base_paths() {
    let git = FakeLaneBackend::default()
        .with_meta("old-main", "old base", 1_700_000_001)
        .with_ancestry_path(Some("old-main"), "main-oid", &[]);
    let lanes = vec![
        lane("old", Some("old-main"), 2, false),
        lane("orphan", None, 1, false),
    ];
    let mut cache = HashMap::new();

    let groups =
        build_lane_groups(&git, lanes, "main-oid", Order::Newest, false, &mut cache).unwrap();

    assert_eq!(groups.len(), 2);
    assert!(groups.iter().all(|group| group.main_distance.is_none()));
    assert!(groups.iter().any(|group| group.base_oid.is_none()));
}

#[test]
fn build_lane_groups_propagates_main_distance_path_errors() {
    let git = FakeLaneBackend::default().with_meta("old-main", "old base", 1_700_000_001);
    let lanes = vec![lane("old", Some("old-main"), 2, false)];
    let mut cache = HashMap::new();

    let error =
        build_lane_groups(&git, lanes, "main-oid", Order::Newest, false, &mut cache).unwrap_err();

    assert_test_fixture_error(error, "ancestry path");
}

#[test]
fn lane_group_order_prefers_main_distance_before_timestamp() {
    let mut groups = [
        lane_group(
            Some("far-main"),
            Some(3),
            vec![lane("newer", None, 100, false)],
        ),
        lane_group(
            Some("near-main"),
            Some(1),
            vec![lane("older", None, 1, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("near-main".to_string()));
    assert_eq!(groups[1].base_oid, Some("far-main".to_string()));
}

#[test]
fn lane_group_order_uses_fallback_after_equal_main_distances() {
    let mut groups = [
        lane_group(
            Some("older-main"),
            Some(2),
            vec![lane("older", None, 1, false)],
        ),
        lane_group(
            Some("newer-main"),
            Some(2),
            vec![lane("newer", None, 100, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("newer-main".to_string()));
    assert_eq!(groups[1].base_oid, Some("older-main".to_string()));
}

#[test]
fn lane_group_order_uses_timestamp_after_distance_ties() {
    let mut groups = [
        lane_group(
            Some("older"),
            None,
            vec![lane("older-head", None, 1, false)],
        ),
        lane_group(
            Some("newer"),
            None,
            vec![lane("newer-head", None, 100, false)],
        ),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("newer".to_string()));
    assert_eq!(groups[1].base_oid, Some("older".to_string()));
}

#[test]
fn lane_group_order_uses_base_oid_after_timestamp_ties() {
    let mut groups = [
        lane_group(Some("base-b"), None, vec![lane("b", None, 10, false)]),
        lane_group(Some("base-a"), None, vec![lane("a", None, 10, false)]),
    ];

    groups.sort_by(|lhs, rhs| lane_group_order(lhs, rhs, Order::Newest));

    assert_eq!(groups[0].base_oid, Some("base-a".to_string()));
    assert_eq!(groups[1].base_oid, Some("base-b".to_string()));
}

#[test]
fn counts_each_branch_point_from_previous_visible_stack_point() {
    let mut points_by_oid = HashMap::new();
    points_by_oid.insert("a".to_string(), point_ref("a", &["feature/one"]));
    points_by_oid.insert("b".to_string(), point_ref("b", &["feature/two"]));
    let lane_path = LanePath {
        head_oid: "b".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["a".to_string(), "b".to_string()],
    };

    let points = branch_points_for_path(&lane_path, &points_by_oid, &[]);

    assert_eq!(
        points,
        vec![
            BranchPointOnPath {
                point: point_ref("b", &["feature/two"]),
                commit_count: 1,
            },
            BranchPointOnPath {
                point: point_ref("a", &["feature/one"]),
                commit_count: 1,
            },
        ]
    );
}

#[test]
fn excludes_rewritten_commits_from_visible_branch_point_counts() {
    let mut points_by_oid = HashMap::new();
    points_by_oid.insert(
        "replacement".to_string(),
        point_ref("replacement", &["base"]),
    );
    points_by_oid.insert("child".to_string(), point_ref("child", &["feature/child"]));
    let lane_path = LanePath {
        head_oid: "child".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "child".to_string()],
    };
    let rewritten = vec![RewrittenCommitRef {
        oid: "old".to_string(),
        replacement_oid: "replacement".to_string(),
    }];

    let points = branch_points_for_path(&lane_path, &points_by_oid, &rewritten);

    assert_eq!(
        points,
        vec![BranchPointOnPath {
            point: point_ref("child", &["feature/child"]),
            commit_count: 1,
        }]
    );
}

#[test]
fn detects_rewritten_commits_whose_current_target_is_selected() {
    let git = FakeLaneBackend::default()
        .with_revset("current(old)", true, &["replacement"])
        .with_revset("current(other)", true, &["other"]);
    let points_by_oid = HashMap::from([
        (
            "replacement".to_string(),
            point_ref("replacement", &["base"]),
        ),
        ("child".to_string(), point_ref("child", &["feature/child"])),
    ]);
    let lane_path = LanePath {
        head_oid: "child".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "other".to_string(), "child".to_string()],
    };

    let rewritten = rewritten_commits_for_path(&git, &lane_path, &points_by_oid).unwrap();

    assert_eq!(
        rewritten,
        vec![RewrittenCommitRef {
            oid: "old".to_string(),
            replacement_oid: "replacement".to_string(),
        }]
    );
}

#[test]
fn ignores_rewritten_candidates_without_exactly_one_current_target() {
    let git = FakeLaneBackend::default()
        .with_revset("current(old)", true, &[])
        .with_revset("current(other)", true, &["first", "second"]);
    let points_by_oid = HashMap::from([(
        "replacement".to_string(),
        point_ref("replacement", &["base"]),
    )]);
    let lane_path = LanePath {
        head_oid: "replacement".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "other".to_string()],
    };

    let rewritten = rewritten_commits_for_path(&git, &lane_path, &points_by_oid).unwrap();

    assert!(rewritten.is_empty());
}

#[test]
fn rewritten_commit_detection_propagates_current_query_errors() {
    let git = FakeLaneBackend::default();
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "head".to_string()],
    };

    let error = rewritten_commits_for_path(&git, &lane_path, &points_by_oid).unwrap_err();

    assert_test_fixture_error(error, "revset");
}

#[test]
fn builds_lane_from_path_with_metadata_and_current_status() {
    let git = FakeLaneBackend::default()
        .with_meta("a", "first", 10)
        .with_meta("b", "second", 20);
    let points_by_oid = HashMap::from([
        ("a".to_string(), point_ref("a", &["feature/one"])),
        ("b".to_string(), point_ref("b", &["feature/two"])),
    ]);
    let lane_path = LanePath {
        head_oid: "b".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["a".to_string(), "b".to_string()],
    };
    let mut cache = HashMap::new();

    let lane = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(Some("feature/two"), None, Verbosity::Medium, false),
        &mut cache,
    )
    .unwrap()
    .unwrap();

    assert_eq!(lane.head_oid, "b");
    assert_eq!(lane.base_oid, Some("main-oid".to_string()));
    assert!(lane.contains_current);
    assert_eq!(
        lane.branch_points,
        vec![
            point_with_count_at("b", &["feature/two"], 1, "second", 20),
            point_with_count_at("a", &["feature/one"], 1, "first", 10),
        ]
    );
}

#[test]
fn builds_lane_from_path_with_rewritten_commit_marker() {
    let git = FakeLaneBackend::default()
        .with_revset("current(old)", true, &["replacement"])
        .with_meta("child", "child", 20)
        .with_meta("old", "old base", 10)
        .with_meta("replacement", "new base", 30);
    let points_by_oid = HashMap::from([
        (
            "replacement".to_string(),
            point_ref("replacement", &["feature/base"]),
        ),
        ("child".to_string(), point_ref("child", &["feature/child"])),
    ]);
    let lane_path = LanePath {
        head_oid: "child".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "child".to_string()],
    };
    let mut cache = HashMap::new();

    let lane = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(None, None, Verbosity::Medium, true),
        &mut cache,
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        lane.branch_points,
        vec![point_with_count_at(
            "child",
            &["feature/child"],
            1,
            "child",
            20
        )]
    );
    assert_eq!(
        lane.rewritten_commits,
        vec![RewrittenCommit::new(
            CommitMeta::new("old", 10, "old base"),
            CommitMeta::new("replacement", 30, "new base"),
        )]
    );
}

#[test]
fn build_lane_from_path_propagates_rewritten_query_errors() {
    let git = FakeLaneBackend::default();
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "head".to_string()],
    };
    let mut cache = HashMap::new();

    let error = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(None, None, Verbosity::Low, true),
        &mut cache,
    )
    .unwrap_err();

    assert_test_fixture_error(error, "revset");
}

#[test]
fn build_lane_from_path_propagates_rewritten_metadata_errors() {
    let git = FakeLaneBackend::default()
        .with_revset("current(old)", true, &["replacement"])
        .with_meta("head", "head", 20)
        .with_meta("replacement", "new base", 30);
    let points_by_oid = HashMap::from([
        ("head".to_string(), point_ref("head", &["feature/head"])),
        (
            "replacement".to_string(),
            point_ref("replacement", &["feature/base"]),
        ),
    ]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["old".to_string(), "head".to_string()],
    };
    let mut cache = HashMap::new();

    let error = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(None, None, Verbosity::Low, true),
        &mut cache,
    )
    .unwrap_err();

    assert_test_fixture_error(error, "metadata");
}

#[test]
fn build_rewritten_commits_propagates_replacement_metadata_errors() {
    let git = FakeLaneBackend::default().with_meta("old", "old base", 10);
    let rewritten_refs = vec![RewrittenCommitRef {
        oid: "old".to_string(),
        replacement_oid: "replacement".to_string(),
    }];
    let mut cache = HashMap::new();

    let error = build_rewritten_commits(&git, &rewritten_refs, &mut cache).unwrap_err();

    assert_test_fixture_error(error, "metadata");
}

#[test]
fn skips_lane_paths_without_visible_branch_points() {
    let git = FakeLaneBackend::default();
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main".to_string()),
        ancestry_path: vec!["parent".to_string(), "head".to_string()],
    };
    let mut cache = HashMap::new();

    let lane = build_lane_from_path(
        &git,
        &lane_path,
        &HashMap::new(),
        lane_context(None, None, Verbosity::Low, false),
        &mut cache,
    )
    .unwrap();

    assert_eq!(lane, None);
}

#[test]
fn build_lane_from_path_propagates_branch_point_metadata_errors() {
    let git = FakeLaneBackend::default();
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["head".to_string()],
    };
    let mut cache = HashMap::new();

    let error = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(None, None, Verbosity::Medium, false),
        &mut cache,
    )
    .unwrap_err();

    assert_test_fixture_error(error, "metadata");
}

#[test]
fn build_lane_from_path_propagates_head_metadata_errors() {
    let git = FakeLaneBackend::default();
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["head".to_string()],
    };
    let mut cache = HashMap::new();

    let error = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(None, None, Verbosity::Low, false),
        &mut cache,
    )
    .unwrap_err();

    assert_test_fixture_error(error, "metadata");
}

#[test]
fn includes_head_branch_point_when_ancestry_path_does_not_contain_it() {
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: None,
        ancestry_path: vec!["root".to_string(), "parent".to_string()],
    };

    let points = branch_points_for_path(&lane_path, &points_by_oid, &[]);

    assert_eq!(
        points,
        vec![BranchPointOnPath {
            point: point_ref("head", &["feature/head"]),
            commit_count: 2,
        }]
    );
}

#[test]
fn counts_head_fallback_as_one_commit_for_empty_ancestry_paths() {
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: None,
        ancestry_path: Vec::new(),
    };

    let points = branch_points_for_path(&lane_path, &points_by_oid, &[]);

    assert_eq!(
        points,
        vec![BranchPointOnPath {
            point: point_ref("head", &["feature/head"]),
            commit_count: 1,
        }]
    );
}

#[test]
fn marks_lane_current_when_detached_head_matches_branch_point() {
    let git = FakeLaneBackend::default().with_meta("head", "detached", 10);
    let points_by_oid = HashMap::from([("head".to_string(), point_ref("head", &["feature/head"]))]);
    let lane_path = LanePath {
        head_oid: "head".to_string(),
        base_oid: Some("main-oid".to_string()),
        ancestry_path: vec!["head".to_string()],
    };
    let mut cache = HashMap::new();

    let lane = build_lane_from_path(
        &git,
        &lane_path,
        &points_by_oid,
        lane_context(Some("other"), Some("head"), Verbosity::Low, false),
        &mut cache,
    )
    .unwrap()
    .unwrap();

    assert!(lane.contains_current);
}

#[test]
fn builds_lanes_from_backend_facts() {
    let revset = "((draft()) & branches()) - public()";
    let git = FakeLaneBackend::default()
        .with_revset("main()", false, &["main-oid"])
        .with_branch_names(
            revset,
            false,
            &["feature/one", "feature/two", "chore/other"],
        )
        .with_local_branches(&[
            ("a", &["feature/one"]),
            ("b", &["feature/two"]),
            ("c", &["chore/other"]),
            ("main-oid", &["main"]),
        ])
        .with_revset(&format!("heads({revset})"), false, &["b", "c"])
        .with_head(Some("b"), Some("feature/two"))
        .with_main_name("main")
        .with_meta("b", "second", 1_700_000_002)
        .with_meta("c", "third", 1_700_000_001)
        .with_merge_base("main-oid", "b", Some("main-oid"))
        .with_ancestry_path(Some("main-oid"), "b", &["a", "b"])
        .with_merge_base("main-oid", "c", Some("main-oid"))
        .with_ancestry_path(Some("main-oid"), "c", &["c"]);
    let args = RuntimeOptions {
        revset: "draft()".to_string(),
        hidden: false,
        debug: false,
        verbosity: Verbosity::Low,
        backend: Backend::Gix,
        order: Order::Newest,
        colour_mode: ColourMode::Never,
        palette: Palette::Classic,
        layout: Layout::Inline,
    };
    let mut cache = HashMap::new();

    let BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
    } = build_lanes(&git, &args, &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(main_oid, "main-oid");
    assert_eq!(repository.main_name, "main");
    assert_eq!(lanes.len(), 2);
    assert_eq!(lanes[0].head_oid, "b");
    assert!(lanes[0].contains_current);
    assert_eq!(
        lanes[0].branch_points,
        vec![point("b", &["feature/two"]), point("a", &["feature/one"])]
    );
    assert_eq!(lanes[1].head_oid, "c");
    assert!(!lanes[1].contains_current);
    assert_eq!(
        git.metadata_requests(),
        vec![vec!["b".to_string(), "c".to_string()]]
    );
}

#[test]
fn build_lanes_skips_head_paths_without_visible_branch_points() {
    let revset = "((draft()) & branches()) - public()";
    let git = FakeLaneBackend::default()
        .with_revset("main()", false, &["main-oid"])
        .with_branch_names(revset, false, &["feature/one"])
        .with_local_branches(&[("visible", &["feature/one"])])
        .with_revset(&format!("heads({revset})"), false, &["visible", "hidden"])
        .with_meta("hidden", "hidden", 1_700_000_001)
        .with_meta("visible", "visible", 1_700_000_002)
        .with_merge_base("main-oid", "hidden", Some("main-oid"))
        .with_merge_base("main-oid", "visible", Some("main-oid"))
        .with_ancestry_path(Some("main-oid"), "hidden", &["hidden"])
        .with_ancestry_path(Some("main-oid"), "visible", &["visible"]);
    let mut cache = HashMap::new();

    let BuiltLanes::Populated { lanes, .. } =
        build_lanes(&git, &runtime_options(Verbosity::Low), &mut cache).unwrap()
    else {
        panic!("expected populated lanes");
    };

    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].head_oid, "visible");
}

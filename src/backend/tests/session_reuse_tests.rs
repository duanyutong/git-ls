use super::*;
use crate::backend::GitCommand;
use crate::error::GitLsError;
use crate::test_support::MockGit;

struct Failure;
impl GitCommand for Failure {
    fn run(&self, _args: &[&str], _allow_failure: bool) -> Result<String> {
        Err(GitLsError::TestFixture("failed read".to_string()))
    }
}

fn inputs() -> Inputs {
    Inputs {
        upstreams: std::collections::BTreeMap::new(),
        branches: std::collections::BTreeMap::from([(
            "main".to_string(),
            vec!["trunk".to_string()],
        )]),
        configured_main: "trunk".to_string(),
    }
}

#[test]
fn frozen_inputs_and_successful_facts_survive_a_new_session() {
    let git = MockGit::default()
        .with(
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(refname:lstrip=2)",
                "refs/heads",
            ],
            "main\0z\nmain\0a",
        )
        .with(&["merge-base", "main", "tip"], "base")
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "tip",
            ],
            "tip\0\u{31}\0subject\u{1e}",
        );
    assert_eq!(
        Inputs::capture(&git).unwrap().branches["main"],
        vec!["a", "z"]
    );
    let session = Session::with_facts(&git, inputs(), Facts::default());
    assert_eq!(
        session.local_branches_by_oid().unwrap()["main"],
        vec!["trunk"]
    );
    assert_eq!(session.main_branch_name().unwrap(), "trunk");
    assert_eq!(
        session.merge_base("main", "tip").unwrap(),
        Some("base".to_string())
    );
    assert_eq!(
        session.merge_base("main", "tip").unwrap(),
        Some("base".to_string())
    );
    assert_eq!(session.merge_base("tip", "main").unwrap(), None);
    assert_eq!(session.merge_base("tip", "main").unwrap(), None);
    let expected = HashMap::from([("main".to_string(), Some(0))]);
    assert_eq!(
        session
            .main_distances("main", &["main".to_string()])
            .unwrap(),
        expected
    );
    assert_eq!(
        session
            .main_distances("main", &["main".to_string()])
            .unwrap(),
        expected
    );
    let mut cache = HashMap::new();
    session.cache_commit_metas(&["tip"], &mut cache).unwrap();
    assert!(session.has_new_facts());
    let facts = session.into_facts();
    let next = Session::with_facts(&git, inputs(), facts);
    cache.clear();
    next.cache_commit_metas(&["tip"], &mut cache).unwrap();
    assert_eq!(cache["tip"].subject, "subject");
    assert_eq!(
        next.merge_base("main", "tip").unwrap(),
        Some("base".to_string())
    );
    assert_eq!(
        next.main_distances("main", &["main".to_string()]).unwrap(),
        expected
    );
    assert!(!next.has_new_facts());
    let ordinary = Session::new(&git);
    ordinary.merge_base("main", "tip").unwrap();
    ordinary.merge_base("main", "tip").unwrap();
    ordinary
        .main_distances("main", &["main".to_string()])
        .unwrap();
    ordinary
        .main_distances("main", &["main".to_string()])
        .unwrap();
    assert_eq!(next.into_facts().bases.len(), 2);
}

struct MainFailure<'a>(&'a MockGit);
impl GitCommand for MainFailure<'_> {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        if args.first() == Some(&"config") {
            return Failure.run(args, allow_failure);
        }
        self.0.run(args, allow_failure)
    }
}

#[test]
fn errors_are_not_memoised_and_storage_limits_do_not_change_answers() {
    assert!(Inputs::capture(&Failure).is_err());
    let failure = Session::new(&Failure);
    assert!(failure.merge_base("a", "b").is_err());
    assert!(failure.main_distances("a", &["b".to_string()]).is_err());
    assert!(failure.query_revset("current(a)", true).is_err());
    let git = MockGit::default()
        .with(
            &["branchless", "query", "-r", "--hidden", "current(a)"],
            "a",
        )
        .with(
            &[
                "show",
                "-s",
                "--format=%H%x00%ct%x00%s%x1e",
                "--no-walk=unsorted",
                "a",
            ],
            "a\0\u{31}\0subject\u{1e}",
        );
    let mut session = Session::new(&git);
    session.limit = 0;
    assert_eq!(session.query_revset("current(a)", true).unwrap(), vec!["a"]);
    assert_eq!(session.merge_base("a", "b").unwrap(), None);
    assert_eq!(
        session.main_distances("a", &["a".to_string()]).unwrap()["a"],
        Some(0)
    );
    session
        .cache_commit_metas(&["a"], &mut HashMap::new())
        .unwrap();
    session.limit = 1;
    session
        .cache_commit_metas(&["a"], &mut HashMap::new())
        .unwrap();
    let omit = crate::test_support::OmitMetadata {
        inner: &git,
        omitted: "a",
    };
    Session::new(&omit)
        .cache_commit_metas(&["a"], &mut HashMap::new())
        .unwrap();
    assert_eq!(session.into_facts().bases.len(), 0);
    let missing_main = MockGit::default().with(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:lstrip=2)",
            "refs/heads",
        ],
        "",
    );
    // Main-name failures are exercised through a command adapter which fails
    // after a successful reference enumeration.
    assert!(Inputs::capture(&MainFailure(&missing_main)).is_err());
}

#[test]
fn captured_upstreams_remain_stable_and_capture_failures_are_preserved() {
    let git = MockGit::default().with(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:lstrip=2)",
            "refs/heads",
        ],
        "local\0main",
    );
    let mut captured = Inputs::capture(&git).unwrap();
    captured.upstreams.insert(
        "main".into(),
        crate::model::MainReference {
            name: "origin/main".into(),
            oid: "remote".into(),
            cached: true,
        },
    );
    let session = Session::with_facts(&git, captured, Facts::default());
    assert_eq!(session.upstream_branches().unwrap()["main"].oid, "remote");
    assert!(Session::new(&git).upstream_branches().unwrap().is_empty());
    let git = git.with_error(&[
        "for-each-ref",
        "--format=%(refname)%00%(objectname)%00%(upstream)%00%(upstream:remotename)",
    ]);
    assert!(Inputs::capture(&git).is_err());
}

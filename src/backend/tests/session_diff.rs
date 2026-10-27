use super::*;
use crate::error::GitLsError;
use crate::model::DiffStat;
use crate::test_support::MockGit;
use std::collections::VecDeque;

#[derive(Default)]
struct Statistics {
    git: MockGit,
    calls: RefCell<Vec<Vec<DiffPair>>>,
    contexts: RefCell<VecDeque<Option<u64>>>,
    context: Cell<Option<u64>>,
    fail_at: Option<usize>,
    short_at: Option<usize>,
    new_path_at: Option<usize>,
}

impl Statistics {
    fn stable() -> Self {
        Self {
            context: Cell::new(Some(1)),
            ..Self::default()
        }
    }
}

impl DiffStatBackend for Statistics {
    fn diff_stats(&self, pairs: &[DiffPair]) -> Result<DiffBatch> {
        self.calls.borrow_mut().push(pairs.to_vec());
        let call = self.calls.borrow().len();
        if self.fail_at == Some(call) {
            return Err(GitLsError::TestFixture("failed comparison".into()));
        }
        Ok(DiffBatch {
            stats: if self.short_at == Some(call) {
                Vec::new()
            } else {
                vec![
                    DiffStat {
                        added: call as u64,
                        deleted: 0
                    };
                    pairs.len()
                ]
            },
            paths: if self.new_path_at == Some(call) {
                vec!["new".into()]
            } else {
                pairs.iter().map(|pair| pair.head.clone()).collect()
            },
        })
    }
    fn diff_context(&self, _paths: &[String]) -> Option<u64> {
        self.contexts
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| self.context.get())
    }
}

impl BranchlessQueries for Statistics {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.git.query_revset(revset, hidden)
    }
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.git.query_branch_names(revset, hidden)
    }
}
impl RepositoryStateBackend for Statistics {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        self.git.local_branches_by_oid()
    }
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        self.git.current_head_and_branch()
    }
    fn main_branch_name(&self) -> Result<String> {
        self.git.main_branch_name()
    }
}
impl CommitMetadataBackend for Statistics {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        self.git.cache_commit_metas(oids, cache)
    }
}
impl AncestryBackend for Statistics {
    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        self.git.merge_base(main, head)
    }
    fn ancestry_path(&self, base: Option<&str>, head: &str) -> Result<Vec<String>> {
        self.git.ancestry_path(base, head)
    }
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.git.unmerged_tips(main, tips)
    }
    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.git.independent_heads(main, tips)
    }
    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        self.git.main_distances(main, bases)
    }
}

fn pair(head: &str) -> DiffPair {
    DiffPair {
        base: Some("main".into()),
        head: head.into(),
    }
}
fn session(git: &Statistics, facts: Facts) -> Session<'_> {
    Session::with_facts(
        git,
        Inputs {
            branches: std::collections::BTreeMap::default(),
            configured_main: "main".into(),
        },
        facts,
    )
}
fn stored() -> Facts {
    let mut facts = Facts::default();
    facts.diff = DiffFacts {
        context: Some(1),
        paths: vec!["one".into()],
        values: HashMap::from([(
            pair("one").key(),
            DiffStat {
                added: 99,
                deleted: 0,
            },
        )]),
    };
    facts
}

#[test]
fn reuses_successful_statistics_across_sessions_and_extends_the_working_set() {
    let git = Statistics::stable();
    let first = session(&git, Facts::default());
    let batch = first.diff_stats(&[pair("one")]).unwrap();
    assert_eq!(batch.stats[0].added, 2);
    assert_eq!(git.calls.borrow().len(), 2);
    assert!(first.has_new_facts());
    let second = session(&git, first.into_facts());
    assert_eq!(
        second.diff_stats(&[pair("one")]).unwrap().stats,
        batch.stats
    );
    assert!(!second.has_new_facts());
    assert_eq!(git.calls.borrow().len(), 2);
    let extended = second
        .diff_stats(&[pair("two"), pair("one"), pair("two")])
        .unwrap();
    assert_eq!(
        extended
            .stats
            .iter()
            .map(|stat| stat.added)
            .collect::<Vec<_>>(),
        vec![4, 2, 4]
    );
    let facts = second.into_facts();
    assert_eq!(facts.diff.paths, vec!["one", "two"]);
    assert_eq!(facts.diff.values.len(), 2);
    git.context.set(Some(2));
    let third = session(&git, facts);
    assert_eq!(third.diff_stats(&[pair("one")]).unwrap().stats[0].added, 6);
    assert_eq!(third.into_facts().diff.values.len(), 1);
}

#[test]
fn unsupported_context_and_capacity_limits_preserve_live_answers() {
    let git = Statistics::default();
    let first = session(&git, Facts::default());
    assert_eq!(first.diff_stats(&[pair("one")]).unwrap().stats[0].added, 1);
    assert!(!first.has_new_facts());
    assert!(first.into_facts().diff.values.is_empty());
    let invalidated = session(&git, stored());
    assert_eq!(
        invalidated.diff_stats(&[pair("one")]).unwrap().stats[0].added,
        2
    );
    assert!(invalidated.has_new_facts());
    assert!(invalidated.into_facts().diff.values.is_empty());
    let git = Statistics::stable();
    let mut limited = session(&git, Facts::default());
    limited.limit = 0;
    assert_eq!(
        limited.diff_stats(&[pair("one")]).unwrap().stats[0].added,
        2
    );
    assert!(limited.into_facts().diff.values.is_empty());
    let cached = session(&git, stored());
    assert_eq!(cached.diff_stats(&[]).unwrap().stats, Vec::new());
    assert_eq!(cached.diff_context(&[]), Some(1));
}

#[test]
fn mutable_context_or_new_dependencies_prevent_publication() {
    for (contexts, new_path) in [
        (vec![Some(1), Some(2)], None),
        (vec![Some(1), None], None),
        (vec![Some(1), Some(1)], Some(2)),
    ] {
        let git = Statistics {
            contexts: RefCell::new(contexts.into()),
            new_path_at: new_path,
            ..Statistics::stable()
        };
        let first = session(&git, Facts::default());
        assert_eq!(first.diff_stats(&[pair("one")]).unwrap().stats[0].added, 2);
        assert!(!first.has_new_facts());
        assert!(first.into_facts().diff.values.is_empty());
    }
}

#[test]
fn partial_hits_are_recomputed_if_context_changes_during_extension() {
    for contexts in [
        vec![Some(1), None],
        vec![Some(1), Some(1), Some(2)],
        vec![Some(1), Some(1), Some(1), Some(2)],
    ] {
        let git = Statistics {
            contexts: RefCell::new(contexts.into()),
            ..Statistics::stable()
        };
        let first = session(&git, stored());
        let batch = first.diff_stats(&[pair("one"), pair("two")]).unwrap();
        assert_eq!(batch.stats.len(), 2);
        assert!(batch.stats.iter().all(|stat| stat.added != 99));
        assert_eq!(
            git.calls.borrow().last().unwrap(),
            &[pair("one"), pair("two")]
        );
        assert!(first.into_facts().diff.values.is_empty());
    }
}

#[test]
fn failures_and_incomplete_batches_are_never_persisted() {
    for call in [1, 2] {
        for fail in [true, false] {
            let git = Statistics {
                fail_at: fail.then_some(call),
                short_at: (!fail).then_some(call),
                ..Statistics::stable()
            };
            let first = session(&git, Facts::default());
            assert!(first.diff_stats(&[pair("one")]).is_err());
            assert!(!first.has_new_facts());
            assert!(first.into_facts().diff.values.is_empty());
        }
    }
}

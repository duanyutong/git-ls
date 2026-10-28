use super::*;
use crate::backend::{DiffBatch, DiffPair, DiffStatBackend};
use crate::cache::facts::{DiffScope, DiffValue};
use crate::error::GitLsError;
use crate::model::DiffStat;
use crate::test_support::MockGit;
use std::collections::VecDeque;

#[derive(Default)]
enum Preparation {
    #[default]
    Normal,
    ShortPaths,
    ShortTrees,
    FailPaths,
    FailTrees,
    Unresolved,
    RecordTrees,
}

#[derive(Default)]
struct Statistics {
    git: MockGit,
    calls: RefCell<Vec<Vec<DiffPair>>>,
    contexts: RefCell<VecDeque<Option<u64>>>,
    context: Cell<Option<u64>>,
    fail_at: Option<usize>,
    short_at: Option<usize>,
    new_path_at: Option<usize>,
    scope: Cell<u64>,
    attributes: RefCell<HashMap<String, u64>>,
    aliases: HashMap<String, String>,
    preparation: Preparation,
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
    fn diff_tree_pairs(
        &self,
        pairs: &[DiffPair],
        trees: &mut HashMap<String, String>,
    ) -> Result<Option<Vec<DiffPair>>> {
        if matches!(self.preparation, Preparation::FailTrees) {
            return Err(GitLsError::TestFixture("tree failure".into()));
        }
        if matches!(self.preparation, Preparation::Unresolved) {
            return Ok(None);
        }
        if matches!(self.preparation, Preparation::RecordTrees) {
            trees.insert("commit".into(), "tree".into());
        }
        if matches!(self.preparation, Preparation::ShortTrees) {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(
            pairs
                .iter()
                .map(|pair| DiffPair {
                    base: pair.base.clone(),
                    head: self.aliases.get(&pair.head).unwrap_or(&pair.head).clone(),
                })
                .collect(),
        ))
    }
    fn diff_paths(&self, pairs: &[DiffPair]) -> Result<Vec<Vec<String>>> {
        if matches!(self.preparation, Preparation::FailPaths) {
            return Err(GitLsError::TestFixture("path failure".into()));
        }
        if matches!(self.preparation, Preparation::ShortPaths) {
            return Ok(Vec::new());
        }
        Ok(pairs.iter().map(|pair| vec![pair.head.clone()]).collect())
    }
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

    fn diff_snapshot(&self, paths: &[String]) -> Option<crate::backend::DiffContext> {
        let mut context = crate::backend::DiffContext::uniform(self.diff_context(paths)?);
        context.scope = self.scope.get();
        context.attributes = self
            .attributes
            .borrow()
            .iter()
            .filter(|(path, _)| paths.contains(path))
            .map(|(path, value)| (path.clone(), *value))
            .collect();
        Some(context)
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
            upstreams: std::collections::BTreeMap::new(),
            branches: std::collections::BTreeMap::default(),
            configured_main: "main".into(),
        },
        facts,
    )
}
fn stored() -> Facts {
    let mut facts = Facts::default();
    let context = crate::backend::DiffContext::uniform(1);
    facts.diff.scopes.insert(
        0,
        DiffScope {
            used: 0,
            values: HashMap::from([(
                pair("one").key(),
                DiffValue {
                    stat: DiffStat {
                        added: 99,
                        deleted: 0,
                    },
                    paths: vec!["one".into()],
                    fingerprint: context.fingerprint(&["one".into()]),
                    used: 0,
                },
            )]),
        },
    );
    facts
}

#[test]
fn reuses_successful_statistics_across_sessions_and_extends_the_working_set() {
    let git = Statistics::stable();
    let first = session(&git, Facts::default());
    let batch = first.diff_stats(&[pair("one")]).unwrap();
    assert_eq!(batch.stats[0].added, 1);
    assert_eq!(git.calls.borrow().len(), 1);
    assert!(first.has_new_facts());
    let second = session(&git, first.into_facts());
    assert_eq!(
        second.diff_stats(&[pair("one")]).unwrap().stats,
        batch.stats
    );
    assert!(!second.has_new_facts());
    assert_eq!(git.calls.borrow().len(), 1);
    let extended = second
        .diff_stats(&[pair("two"), pair("one"), pair("two")])
        .unwrap();
    assert_eq!(
        extended
            .stats
            .iter()
            .map(|stat| stat.added)
            .collect::<Vec<_>>(),
        vec![2, 1, 2]
    );
    let facts = second.into_facts();
    assert_eq!(facts.diff.scopes[&0].values.len(), 2);
    git.context.set(Some(2));
    let third = session(&git, facts);
    assert_eq!(third.diff_stats(&[pair("one")]).unwrap().stats[0].added, 3);
    assert_eq!(third.into_facts().diff.scopes[&0].values.len(), 2);
}

#[test]
fn unsupported_context_and_capacity_limits_preserve_live_answers() {
    let git = Statistics::default();
    let first = session(&git, Facts::default());
    assert_eq!(first.diff_stats(&[pair("one")]).unwrap().stats[0].added, 1);
    assert!(!first.has_new_facts());
    assert!(first.into_facts().diff.scopes.is_empty());
    let invalidated = session(&git, stored());
    assert_eq!(
        invalidated.diff_stats(&[pair("one")]).unwrap().stats[0].added,
        2
    );
    assert!(!invalidated.has_new_facts());
    let git = Statistics::stable();
    let mut limited = session(&git, Facts::default());
    limited.limit = 0;
    assert_eq!(
        limited.diff_stats(&[pair("one")]).unwrap().stats[0].added,
        1
    );
    assert!(limited.into_facts().diff.scopes[&0].values.is_empty());
    let cached = session(&git, stored());
    assert_eq!(cached.diff_stats(&[]).unwrap().stats, Vec::new());
    assert_eq!(cached.diff_context(&[]), Some(1));
}

#[test]
fn mutable_context_or_new_dependencies_prevent_publication() {
    for (contexts, new_path) in [
        (vec![Some(1), Some(2)], None),
        (vec![Some(1), None], None),
        (vec![Some(1), Some(1)], Some(1)),
    ] {
        let git = Statistics {
            contexts: RefCell::new(contexts.into()),
            new_path_at: new_path,
            ..Statistics::stable()
        };
        let first = session(&git, Facts::default());
        assert_eq!(first.diff_stats(&[pair("one")]).unwrap().stats[0].added, 1);
        assert!(!first.has_new_facts());
        assert!(first.into_facts().diff.scopes.is_empty());
    }
}

#[test]
fn partial_hits_are_recomputed_if_context_changes_during_extension() {
    for after in [None, Some(2)] {
        let git = Statistics {
            contexts: RefCell::new(vec![Some(1), after].into()),
            ..Statistics::stable()
        };
        let first = session(&git, stored());
        let batch = first.diff_stats(&[pair("one"), pair("two")]).unwrap();
        assert!(batch.stats.iter().all(|stat| stat.added == 2));
        assert_eq!(
            git.calls.borrow().last().unwrap(),
            &[pair("one"), pair("two")]
        );
        assert!(!first.has_new_facts());
    }
}

#[test]
fn failures_and_incomplete_batches_are_never_persisted() {
    for fail in [true, false] {
        let git = Statistics {
            fail_at: fail.then_some(1),
            short_at: (!fail).then_some(1),
            ..Statistics::stable()
        };
        let first = session(&git, Facts::default());
        assert!(first.diff_stats(&[pair("one")]).is_err());
        assert!(!first.has_new_facts());
        assert!(first.into_facts().diff.scopes.is_empty());
    }
}

#[test]
fn equal_trees_do_not_require_context_or_numeric_comparisons() {
    let git = Statistics::default();
    let first = session(&git, Facts::default());
    let batch = first
        .diff_stats(&[DiffPair {
            base: Some("one".into()),
            head: "one".into(),
        }])
        .unwrap();
    assert_eq!(batch.stats, vec![DiffStat::default()]);
    assert!(git.calls.borrow().is_empty());
}

#[test]
fn oversized_dependencies_bypass_admission_without_a_second_comparison() {
    let git = Statistics::stable();
    let first = session(&git, Facts::default());
    let huge = pair(&"x".repeat(usize::try_from(crate::cache::ENTRY_LIMIT).unwrap() + 1));
    assert_eq!(first.diff_stats(&[huge]).unwrap().stats[0].added, 1);
    assert!(first.into_facts().diff.scopes.is_empty());
    assert_eq!(git.calls.borrow().len(), 1);
}

#[test]
fn linked_contexts_and_unrequested_attribute_changes_preserve_independent_results() {
    let git = Statistics::stable();
    let first = session(&git, Facts::default());
    first.diff_stats(&[pair("one"), pair("two")]).unwrap();
    git.scope.set(1);
    let second = session(&git, first.into_facts());
    second.diff_stats(&[pair("one")]).unwrap();
    git.scope.set(0);
    git.attributes.borrow_mut().insert("two".into(), 99);
    let third = session(&git, second.into_facts());
    assert_eq!(third.diff_stats(&[pair("one")]).unwrap().stats[0].added, 1);
    assert!(!third.has_new_facts());
    assert_eq!(git.calls.borrow().len(), 2);
    let changed = third.diff_stats(&[pair("two")]).unwrap();
    assert_eq!(changed.stats[0].added, 3);
}

#[test]
fn identical_tree_pairs_reuse_counts_after_commit_identity_changes() {
    let git = Statistics {
        aliases: HashMap::from([("amended".into(), "one".into())]),
        ..Statistics::stable()
    };
    let first = session(&git, Facts::default());
    first.diff_stats(&[pair("one")]).unwrap();
    let second = session(&git, first.into_facts());
    assert_eq!(
        second.diff_stats(&[pair("amended")]).unwrap().stats[0].added,
        1
    );
    assert_eq!(git.calls.borrow().len(), 1);
}

#[test]
fn preparation_must_return_complete_tree_and_path_records() {
    for preparation in [Preparation::ShortPaths, Preparation::ShortTrees] {
        let git = Statistics {
            preparation,
            ..Statistics::stable()
        };
        assert!(
            session(&git, Facts::default())
                .diff_stats(&[pair("one")])
                .is_err()
        );
        assert!(git.calls.borrow().is_empty());
    }
}

#[test]
fn preparation_failures_and_live_fallbacks_preserve_the_error_and_fact_contract() {
    for preparation in [Preparation::FailPaths, Preparation::FailTrees] {
        let git = Statistics {
            preparation,
            ..Statistics::stable()
        };
        assert!(
            session(&git, Facts::default())
                .diff_stats(&[pair("one")])
                .is_err()
        );
    }
    let git = Statistics {
        preparation: Preparation::RecordTrees,
        ..Statistics::stable()
    };
    let first = session(&git, Facts::default());
    first.diff_stats(&[pair("one")]).unwrap();
    assert!(first.has_new_facts());
    assert_eq!(first.into_facts().diff.trees["commit"], "tree");
    let git = Statistics {
        preparation: Preparation::Unresolved,
        ..Statistics::stable()
    };
    assert_eq!(
        session(&git, Facts::default())
            .diff_stats(&[pair("one")])
            .unwrap()
            .stats[0]
            .added,
        1
    );
    let git = Statistics {
        fail_at: Some(1),
        ..Statistics::default()
    };
    assert!(
        session(&git, Facts::default())
            .diff_stats(&[pair("one")])
            .is_err()
    );
    let git = Statistics {
        fail_at: Some(1),
        ..Statistics::default()
    };
    let huge = pair(&"x".repeat(usize::try_from(crate::cache::ENTRY_LIMIT).unwrap() + 1));
    assert!(session(&git, Facts::default()).diff_stats(&[huge]).is_err());
    let git = Statistics {
        contexts: RefCell::new(vec![Some(1), Some(2)].into()),
        fail_at: Some(2),
        ..Statistics::stable()
    };
    assert!(
        session(&git, stored())
            .diff_stats(&[pair("one"), pair("two")])
            .is_err()
    );
}

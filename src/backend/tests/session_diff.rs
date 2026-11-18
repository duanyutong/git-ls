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

#[derive(Clone, Copy)]
enum Incremental {
    Supported,
    Short,
    Uncovered,
    Missing,
    Failure,
    Underflow,
}

#[derive(Default)]
struct Statistics {
    git: MockGit,
    incremental: Option<Incremental>,
    incremental_calls: Cell<usize>,
    calls: RefCell<Vec<Vec<DiffPair>>>,
    contexts: RefCell<VecDeque<Option<u64>>>,
    context: Cell<Option<u64>>,
    fail_at: Option<usize>,
    short_at: Option<usize>,
    new_path_at: Option<usize>,
    scope: Cell<u64>,
    attributes: RefCell<HashMap<String, u64>>,
    aliases: HashMap<String, String>,
    paths: HashMap<String, Vec<String>>,
    path_calls: RefCell<Vec<Vec<DiffPair>>>,
    unavailable_trees: std::collections::HashSet<String>,
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
    fn incremental_stats(
        &self,
        _previous: &DiffPair,
        _current: &DiffPair,
        paths: &[String],
    ) -> Result<Option<DiffBatch>> {
        self.incremental_calls.set(self.incremental_calls.get() + 1);
        let Some(mode) = self.incremental else {
            return Ok(None);
        };
        if matches!(mode, Incremental::Failure) {
            return Err(GitLsError::TestFixture("optional failure".into()));
        }
        Ok(Some(DiffBatch {
            stats: if matches!(mode, Incremental::Short) {
                vec![]
            } else {
                vec![
                    DiffStat {
                        added: if matches!(mode, Incremental::Underflow) {
                            2
                        } else {
                            1
                        },
                        deleted: 0,
                    },
                    DiffStat {
                        added: 4,
                        deleted: 0,
                    },
                ]
            },
            paths: if matches!(mode, Incremental::Uncovered) {
                vec!["uncovered".into()]
            } else if matches!(mode, Incremental::Missing) {
                Vec::new()
            } else {
                paths.to_vec()
            },
        }))
    }

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
        self.path_calls.borrow_mut().push(pairs.to_vec());
        if pairs.iter().any(|pair| {
            pair.base
                .as_ref()
                .is_some_and(|base| self.unavailable_trees.contains(base))
        }) {
            return Err(GitLsError::TestFixture(
                "unavailable historical tree".into(),
            ));
        }
        if matches!(self.preparation, Preparation::FailPaths) {
            return Err(GitLsError::TestFixture("path failure".into()));
        }
        if matches!(self.preparation, Preparation::ShortPaths) {
            return Ok(Vec::new());
        }
        Ok(pairs
            .iter()
            .map(|pair| {
                self.paths
                    .get(&pair.head)
                    .cloned()
                    .unwrap_or_else(|| vec![pair.head.clone()])
            })
            .collect())
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
                pairs
                    .iter()
                    .flat_map(|pair| {
                        self.paths
                            .get(&pair.head)
                            .cloned()
                            .unwrap_or_else(|| vec![pair.head.clone()])
                    })
                    .collect()
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
                    paths: std::rc::Rc::new(vec!["one".into()]),
                    fingerprint: context.fingerprint(&["one".into()]),
                    configuration: context.configuration,
                    dependency_depth: 0,
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

#[test]
fn unavailable_context_preserves_live_errors_without_retaining_counts() {
    for short in [false, true] {
        let git = Statistics {
            fail_at: (!short).then_some(1),
            short_at: short.then_some(1),
            ..Statistics::default()
        };
        let current = session(&git, Facts::default());
        assert!(current.diff_stats(&[pair("one")]).is_err());
        assert!(!current.has_new_facts());
        assert!(current.into_facts().diff.scopes.is_empty());
        assert_eq!(git.calls.borrow().len(), 1);
    }
}

#[test]
fn oversized_comparisons_preserve_valid_small_cache_hits() {
    let git = Statistics {
        paths: HashMap::from([("huge".into(), vec!["x".repeat(1024 * 1024 + 1)])]),
        ..Statistics::stable()
    };
    let current = session(&git, stored());
    let batch = current.diff_stats(&[pair("one"), pair("huge")]).unwrap();
    assert_eq!(batch.stats[0].added, 99);
    assert_eq!(batch.stats[1].added, 1);
    assert_eq!(*git.calls.borrow(), vec![vec![pair("huge")]]);
    assert!(!current.has_new_facts());
    assert_eq!(current.into_facts().diff.scopes[&0].values.len(), 1);
}

#[test]
fn oversized_dependency_unions_admit_a_validated_subset_without_republishing() {
    let git = Statistics {
        paths: HashMap::from([
            ("alpha".into(), vec!["x".repeat(600_000)]),
            ("beta".into(), vec!["y".repeat(600_000)]),
        ]),
        ..Statistics::stable()
    };
    let first = session(&git, Facts::default());
    let pairs = [pair("alpha"), pair("beta")];
    first.diff_stats(&pairs).unwrap();
    let facts = first.into_facts();
    assert_eq!(facts.diff.scopes[&0].values.len(), 1);
    assert!(
        facts.diff.scopes[&0]
            .values
            .contains_key(&pair("alpha").key())
    );
    assert!(
        !facts.diff.scopes[&0]
            .values
            .contains_key(&pair("beta").key())
    );
    let second = session(&git, facts);
    let batch = second.diff_stats(&pairs).unwrap();
    assert_eq!(batch.stats[0].added, 1);
    assert_eq!(batch.stats[1].added, 2);
    assert_eq!(git.calls.borrow().last().unwrap(), &vec![pair("beta")]);
    assert!(!second.has_new_facts());
}

fn tree_pair(base: u8, head: u8) -> DiffPair {
    DiffPair {
        base: Some(format!("{base:040x}")),
        head: format!("{head:040x}"),
    }
}

#[test]
fn dependency_discovery_uses_deduplicated_transitions_and_refreshes_long_chains() {
    let git = Statistics::stable();
    let initial = session(&git, Facts::default());
    initial
        .diff_stats(&[tree_pair(1, 2), tree_pair(4, 2)])
        .unwrap();
    let next = session(&git, initial.into_facts());
    next.diff_stats(&[tree_pair(1, 3), tree_pair(4, 3)])
        .unwrap();
    assert_eq!(
        git.path_calls.borrow().last().unwrap(),
        &vec![tree_pair(2, 3)]
    );
    assert_eq!(next.diff_transitions.borrow().len(), 2);
    let mut facts = next.into_facts();
    assert_eq!(
        facts.diff.scopes[&0].values[&tree_pair(1, 3).key()].dependency_depth,
        1
    );
    for head in 4..=12 {
        let current = session(&git, facts);
        current.diff_stats(&[tree_pair(1, head)]).unwrap();
        facts = current.into_facts();
    }
    // The most recent proof reaches its bound, then direct discovery resets it.
    assert!(
        git.path_calls
            .borrow()
            .iter()
            .skip(2)
            .any(|pairs| pairs == &vec![tree_pair(1, 11)])
    );
    assert_eq!(
        facts.diff.scopes[&0].values[&tree_pair(1, 12).key()].dependency_depth,
        1
    );
}

#[test]
fn mixed_discovery_and_missing_historical_objects_preserve_complete_results() {
    let mut git = Statistics::stable();
    let first = session(&git, Facts::default());
    first.diff_stats(&[tree_pair(1, 2)]).unwrap();
    let facts = first.into_facts();
    let mixed = session(&git, facts.clone());
    mixed
        .diff_stats(&[tree_pair(1, 3), tree_pair(4, 5)])
        .unwrap();
    let calls = git.path_calls.borrow();
    assert_eq!(calls[calls.len() - 2], vec![tree_pair(2, 3)]);
    assert_eq!(calls.last().unwrap(), &vec![tree_pair(4, 5)]);
    drop(calls);
    drop(mixed);
    git.unavailable_trees.insert(format!("{:040x}", 2));
    let current = session(&git, facts.clone());
    assert!(current.diff_stats(&[tree_pair(1, 3)]).is_ok());
    assert!(current.diff_transitions.borrow().is_empty());
    assert_eq!(
        git.path_calls.borrow().last().unwrap(),
        &vec![tree_pair(1, 3)]
    );
    assert_eq!(
        current.into_facts().diff.scopes[&0].values[&tree_pair(1, 3).key()].dependency_depth,
        0
    );
    git.unavailable_trees.clear();
    git.preparation = Preparation::ShortPaths;
    assert!(session(&git, facts).diff_stats(&[tree_pair(1, 3)]).is_err());
}

#[test]
fn malformed_historical_keys_are_ignored_and_distinct_context_proofs_remain_reusable() {
    let git = Statistics::stable();
    let mut facts = stored();
    let value = facts.diff.scopes[&0].values[&pair("one").key()].clone();
    for key in [
        "not-a-pair".to_owned(),
        format!("{:040x}:{}", 1, "z".repeat(40)),
    ] {
        facts
            .diff
            .scopes
            .get_mut(&0)
            .unwrap()
            .values
            .insert(key, value.clone());
    }
    let current = session(&git, facts);
    current.diff_stats(&[tree_pair(1, 3)]).unwrap();
    assert_eq!(
        git.path_calls.borrow().last().unwrap(),
        &vec![tree_pair(1, 3)]
    );
    let mut facts = current.into_facts();
    let mut other = facts.diff.scopes[&0].clone();
    let extra = other.values.get_mut(&tree_pair(1, 3).key()).unwrap();
    extra.paths = std::rc::Rc::new(vec!["extra".into(), format!("{:040x}", 3)]);
    let mut context = crate::backend::DiffContext::uniform(1);
    context.scope = 1;
    extra.fingerprint = context.fingerprint(&extra.paths);
    facts.diff.scopes.insert(1, other);
    git.scope.set(1);
    let calls = git.calls.borrow().len();
    let reused = session(&git, facts);
    reused.diff_stats(&[tree_pair(1, 3)]).unwrap();
    assert_eq!(git.calls.borrow().len(), calls);
}

#[test]
fn sha256_tree_identifiers_support_transition_dependency_reuse() {
    let git = Statistics::stable();
    let pair = |head: u8| DiffPair {
        base: Some(format!("{:064x}", 1)),
        head: format!("{head:064x}"),
    };
    let first = session(&git, Facts::default());
    first.diff_stats(&[pair(2)]).unwrap();
    let next = session(&git, first.into_facts());
    next.diff_stats(&[pair(3)]).unwrap();
    assert_eq!(
        git.path_calls.borrow().last().unwrap(),
        &vec![DiffPair {
            base: Some(format!("{:064x}", 2)),
            head: format!("{:064x}", 3),
        }]
    );
}

fn incremental_fixture(
    mode: Option<Incremental>,
    count: usize,
    changed: Vec<String>,
) -> Statistics {
    Statistics {
        incremental: mode,
        paths: HashMap::from([
            (
                format!("{:040x}", 2),
                (0..count).map(|index| format!("path-{index:04}")).collect(),
            ),
            (format!("{:040x}", 3), changed),
        ]),
        ..Statistics::stable()
    }
}

#[test]
fn incremental_correction_uses_validated_totals_and_optional_failures_fall_back() {
    for mode in [
        Some(Incremental::Supported),
        Some(Incremental::Short),
        Some(Incremental::Uncovered),
        Some(Incremental::Missing),
        Some(Incremental::Failure),
        Some(Incremental::Underflow),
        None,
    ] {
        let git = incremental_fixture(mode, 64, vec!["path-0000".into()]);
        let first = session(&git, Facts::default());
        first.diff_stats(&[tree_pair(1, 2)]).unwrap();
        let next = session(&git, first.into_facts());
        let answer = next.diff_stats(&[tree_pair(1, 3)]).unwrap();
        let supported = matches!(mode, Some(Incremental::Supported));
        assert_eq!(answer.stats[0].added, if supported { 4 } else { 2 });
        assert_eq!(git.calls.borrow().len(), if supported { 1 } else { 2 });
        assert_eq!(git.incremental_calls.get(), 1);
    }
}

#[test]
fn incremental_correction_enforces_benefit_limits_and_current_context() {
    for (count, changed) in [
        (63, vec!["path-0000".into()]),
        (64, vec![]),
        (
            1024,
            (0..129).map(|index| format!("path-{index:04}")).collect(),
        ),
        (
            64,
            (0..16).map(|index| format!("path-{index:04}")).collect(),
        ),
        (64, vec!["long".repeat(9000)]),
    ] {
        let git = incremental_fixture(Some(Incremental::Supported), count, changed);
        let first = session(&git, Facts::default());
        first.diff_stats(&[tree_pair(1, 2)]).unwrap();
        session(&git, first.into_facts())
            .diff_stats(&[tree_pair(1, 3)])
            .unwrap();
        assert_eq!(git.incremental_calls.get(), 0);
        assert_eq!(git.calls.borrow().len(), 2);
    }
    for scenario in 0..3 {
        let git = incremental_fixture(Some(Incremental::Supported), 64, vec!["path-0000".into()]);
        let first = session(&git, Facts::default());
        first.diff_stats(&[tree_pair(1, 2)]).unwrap();
        match scenario {
            0 => git.context.set(Some(2)),
            1 => git.scope.set(1),
            _ => {
                git.attributes.borrow_mut().insert("path-0001".into(), 42);
            }
        }
        session(&git, first.into_facts())
            .diff_stats(&[tree_pair(1, 3)])
            .unwrap();
        assert_eq!(git.incremental_calls.get(), 0);
    }
}

#[test]
fn incremental_context_race_discards_the_correction_and_recalculates_live() {
    let git = incremental_fixture(Some(Incremental::Supported), 64, vec!["path-0000".into()]);
    let first = session(&git, Facts::default());
    first.diff_stats(&[tree_pair(1, 2)]).unwrap();
    git.contexts.borrow_mut().extend([Some(1), Some(2)]);
    let next = session(&git, first.into_facts());
    assert_eq!(
        next.diff_stats(&[tree_pair(1, 3)]).unwrap().stats[0].added,
        2
    );
    assert_eq!(git.incremental_calls.get(), 1);
    assert!(
        !next.into_facts().diff.scopes[&0]
            .values
            .contains_key(&tree_pair(1, 3).key())
    );
}

#[test]
fn multiple_transition_groups_preserve_order_and_required_current_failures() {
    let mut git = Statistics::stable();
    let first = session(&git, Facts::default());
    first
        .diff_stats(&[tree_pair(1, 2), tree_pair(4, 5)])
        .unwrap();
    let facts = first.into_facts();
    let pairs = [
        tree_pair(1, 3),
        tree_pair(1, 6),
        tree_pair(4, 7),
        tree_pair(8, 9),
    ];
    let mixed = session(&git, facts.clone());
    assert_eq!(mixed.diff_stats(&pairs).unwrap().stats.len(), pairs.len());
    let calls = git.path_calls.borrow();
    assert_eq!(
        calls[calls.len() - 2],
        vec![tree_pair(2, 3), tree_pair(2, 6), tree_pair(5, 7)]
    );
    assert_eq!(calls.last().unwrap(), &vec![tree_pair(8, 9)]);
    drop(calls);
    drop(mixed);
    git.unavailable_trees.insert(format!("{:040x}", 8));
    assert!(session(&git, facts).diff_stats(&pairs).is_err());
    assert_eq!(git.calls.borrow().len(), 2);
}

#[test]
fn incremental_correction_requires_a_total_in_the_current_scope() {
    let git = incremental_fixture(Some(Incremental::Supported), 64, vec!["path-0000".into()]);
    let first = session(&git, Facts::default());
    first.diff_stats(&[tree_pair(1, 2)]).unwrap();
    let mut facts = first.into_facts();
    facts.diff.scopes.insert(1, DiffScope::default());
    git.scope.set(1);
    let answer = session(&git, facts).diff_stats(&[tree_pair(1, 3)]).unwrap();
    assert_eq!(answer.stats[0].added, 2);
    assert_eq!(git.incremental_calls.get(), 0);
    assert_eq!(git.calls.borrow().len(), 2);
}

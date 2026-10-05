use std::cell::RefCell;
use std::collections::HashMap;

use super::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, GitBackend, RepositoryStateBackend,
};
use crate::error::Result;
use crate::model::CommitMeta;

const QUERY_LIMIT: usize = 65_536;

/// Reuses completed, directional queries within one invocation. Branchless state
/// is deliberately never persisted: local refs do not identify its event log.
pub(crate) struct Session<'a> {
    inner: &'a dyn GitBackend,
    rewrites: RefCell<HashMap<String, Vec<String>>>,
    facts: RefCell<Facts>,
    inputs: Option<Inputs>,
    limit: usize,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Facts {
    pub(crate) bases: HashMap<String, Option<String>>,
    pub(crate) distances: HashMap<String, Option<usize>>,
    pub(crate) metadata: HashMap<String, CommitMeta>,
    #[serde(default)]
    recency: HashMap<String, u64>,
    #[serde(default)]
    clock: u64,
}

impl Facts {
    fn touch(&mut self, key: &str) {
        self.clock = self.clock.saturating_add(1);
        self.recency.insert(key.to_string(), self.clock);
    }

    pub(crate) fn compact(&mut self, limit: usize) {
        fn retain<T>(map: &mut HashMap<String, T>, recency: &HashMap<String, u64>, limit: usize) {
            if map.len() <= limit {
                return;
            }
            let mut newest = std::collections::BinaryHeap::new();
            for key in map.keys() {
                newest.push(std::cmp::Reverse((
                    recency.get(key).copied().unwrap_or(0),
                    key,
                )));
                if newest.len() > limit {
                    newest.pop();
                }
            }
            let retained: std::collections::HashSet<_> = newest
                .into_iter()
                .map(|std::cmp::Reverse((_, key))| key.clone())
                .collect();
            map.retain(|key, _| retained.contains(key));
        }
        retain(&mut self.bases, &self.recency, limit);
        retain(&mut self.distances, &self.recency, limit);
        retain(&mut self.metadata, &self.recency, limit);
        self.recency.retain(|key, _| {
            self.bases.contains_key(key)
                || self.distances.contains_key(key)
                || self.metadata.contains_key(key)
        });
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct Inputs {
    pub(crate) branches: std::collections::BTreeMap<String, Vec<String>>,
    pub(crate) configured_main: String,
}

impl Inputs {
    pub(crate) fn capture(git: &dyn GitBackend) -> Result<Self> {
        let mut branches: std::collections::BTreeMap<_, _> =
            git.local_branches_by_oid()?.into_iter().collect();
        for names in branches.values_mut() {
            names.sort();
        }
        Ok(Self {
            branches,
            configured_main: git.main_branch_name()?,
        })
    }
}

impl<'a> Session<'a> {
    pub(crate) fn with_facts(inner: &'a dyn GitBackend, inputs: Inputs, facts: Facts) -> Self {
        Self {
            inner,
            inputs: Some(inputs),
            limit: QUERY_LIMIT,
            facts: RefCell::new(facts),
            rewrites: RefCell::default(),
        }
    }
    pub(crate) fn into_facts(self) -> Facts {
        self.facts.into_inner()
    }

    pub(crate) fn new(inner: &'a dyn GitBackend) -> Self {
        Self {
            inner,
            rewrites: RefCell::default(),
            facts: RefCell::default(),
            inputs: None,
            limit: QUERY_LIMIT,
        }
    }
}

impl BranchlessQueries for Session<'_> {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        if !hidden || !revset.starts_with("current(") {
            return self.inner.query_revset(revset, hidden);
        }
        if let Some(result) = self.rewrites.borrow().get(revset) {
            crate::diagnostics::count("rewrite_cache_hits", 1);
            return Ok(result.clone());
        }
        let result = self.inner.query_revset(revset, hidden)?;
        if self.rewrites.borrow().len() < self.limit {
            self.rewrites
                .borrow_mut()
                .insert(revset.to_string(), result.clone());
        }
        Ok(result)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        self.inner.query_branch_names(revset, hidden)
    }
}

impl CommitMetadataBackend for Session<'_> {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        {
            let facts = self.facts.borrow();
            for oid in oids {
                if !cache.contains_key(*oid)
                    && let Some(meta) = facts.metadata.get(*oid)
                {
                    cache.insert((*oid).to_string(), meta.clone());
                    crate::diagnostics::count("persistent_metadata_hits", 1);
                }
            }
        }
        self.inner.cache_commit_metas(oids, cache)?;
        let mut facts = self.facts.borrow_mut();
        for oid in oids {
            if self.inputs.is_some() && facts.metadata.contains_key(*oid) {
                facts.touch(oid);
            }
            if facts.metadata.len() < self.limit
                && let Some(meta) = cache.get(*oid)
            {
                facts.metadata.insert((*oid).to_string(), meta.clone());
                if self.inputs.is_some() {
                    facts.touch(oid);
                }
            }
        }
        Ok(())
    }
}

impl RepositoryStateBackend for Session<'_> {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        if let Some(inputs) = &self.inputs {
            return Ok(inputs.branches.clone().into_iter().collect());
        }
        self.inner.local_branches_by_oid()
    }
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        self.inner.current_head_and_branch()
    }
    fn main_branch_name(&self) -> Result<String> {
        if let Some(inputs) = &self.inputs {
            return Ok(inputs.configured_main.clone());
        }
        self.inner.main_branch_name()
    }
}

impl AncestryBackend for Session<'_> {
    fn merge_base(&self, main: &str, head: &str) -> Result<Option<String>> {
        let key = format!("{main}:{head}");
        let cached = self.facts.borrow().bases.get(&key).cloned();
        if let Some(result) = cached {
            if self.inputs.is_some() {
                self.facts.borrow_mut().touch(&key);
            }
            crate::diagnostics::count("merge_base_cache_hits", 1);
            return Ok(result);
        }
        let result = self.inner.merge_base(main, head)?;
        if self.facts.borrow().bases.len() < self.limit {
            let mut facts = self.facts.borrow_mut();
            if self.inputs.is_some() {
                facts.touch(&key);
            }
            facts.bases.insert(key, result.clone());
        }
        Ok(result)
    }
    fn ancestry_path(&self, base: Option<&str>, head: &str) -> Result<Vec<String>> {
        self.inner.ancestry_path(base, head)
    }
    fn unmerged_tips(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.inner.unmerged_tips(main, tips)
    }
    fn independent_heads(&self, main: &str, tips: &[String]) -> Result<Vec<String>> {
        self.inner.independent_heads(main, tips)
    }
    fn main_distances(
        &self,
        main: &str,
        bases: &[String],
    ) -> Result<HashMap<String, Option<usize>>> {
        let mut result = HashMap::new();
        let mut missing = Vec::new();
        {
            let mut facts = self.facts.borrow_mut();
            for base in bases {
                if let Some(distance) = facts.distances.get(&format!("{main}:{base}")) {
                    result.insert(base.clone(), *distance);
                    if self.inputs.is_some() {
                        facts.touch(&format!("{main}:{base}"));
                    }
                    crate::diagnostics::count("distance_cache_hits", 1);
                } else {
                    missing.push(base.clone());
                }
            }
        }
        if !missing.is_empty() {
            let computed = self.inner.main_distances(main, &missing)?;
            let mut facts = self.facts.borrow_mut();
            for (base, distance) in &computed {
                if facts.distances.len() < self.limit {
                    let key = format!("{main}:{base}");
                    if self.inputs.is_some() {
                        facts.touch(&key);
                    }
                    facts.distances.insert(key, *distance);
                }
            }
            result.extend(computed);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockGit;

    #[test]
    fn memoises_unchanged_empty_and_ambiguous_rewrites() {
        for result in ["oid", "", "a\nb"] {
            let git = MockGit::default().with(
                &["branchless", "query", "-r", "--hidden", "current(oid)"],
                result,
            );
            let session = Session::new(&git);
            let first = session.query_revset("current(oid)", true).unwrap();
            assert_eq!(first, session.query_revset("current(oid)", true).unwrap());
            assert_eq!(git.calls().len(), 1);
        }
    }
}

#[cfg(test)]
mod reuse_tests {
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
                    "--format=%(objectname)%00%(refname:short)",
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
                "--format=%(objectname)%00%(refname:short)",
                "refs/heads",
            ],
            "",
        );
        // Main-name failures are exercised through a command adapter which fails
        // after a successful reference enumeration.
        assert!(Inputs::capture(&MainFailure(&missing_main)).is_err());
    }

    #[test]
    fn compaction_retains_recently_reused_facts() {
        let mut facts = Facts::default();
        for key in ["old", "warm", "new"] {
            facts.bases.insert(key.to_string(), None);
            facts.distances.insert(key.to_string(), Some(1));
            facts
                .metadata
                .insert(key.to_string(), CommitMeta::new(key, 0, key));
            facts.touch(key);
        }
        facts.touch("warm");
        facts.recency.insert("discarded".to_string(), 100);
        facts.compact(1);
        assert_eq!(facts.bases.keys().collect::<Vec<_>>(), vec!["warm"]);
        assert_eq!(facts.distances.len(), 1);
        assert_eq!(facts.metadata.len(), 1);
        assert_eq!(facts.recency.len(), 1);
        let mut facts = Facts::default();
        facts.bases.insert("base".to_string(), None);
        facts.bases.insert("untouched".to_string(), None);
        facts.distances.insert("distance".to_string(), None);
        facts.metadata.insert(
            "metadata".to_string(),
            CommitMeta::new("metadata", 0, "meta"),
        );
        for key in ["base", "distance", "metadata", "removed"] {
            facts.touch(key);
        }
        facts.compact(1);
        assert_eq!(facts.recency.len(), 3);
    }
}

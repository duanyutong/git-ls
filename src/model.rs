const DISPLAY_OID_LEN: usize = 7;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DiffStat {
    pub(crate) added: u64,
    pub(crate) deleted: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CommitMeta {
    pub(crate) oid: String,
    pub(crate) short_oid: String,
    pub(crate) subject: String,
    pub(crate) timestamp: i64,
}

impl CommitMeta {
    pub(crate) fn new(oid: impl Into<String>, timestamp: i64, subject: impl Into<String>) -> Self {
        let oid = oid.into();
        Self {
            short_oid: display_short_oid(&oid),
            oid,
            subject: subject.into(),
            timestamp,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BranchPoint {
    pub(crate) oid: String,
    pub(crate) names: Vec<String>,
    pub(crate) annotation: Option<BranchAnnotation>,
    pub(crate) commit_count: usize,
    #[serde(skip)]
    pub(crate) diff_stat: Option<DiffStat>,
}

impl BranchPoint {
    pub(crate) fn new<I, S>(
        oid: impl Into<String>,
        names: I,
        annotation: Option<BranchAnnotation>,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            oid: oid.into(),
            names: names.into_iter().map(Into::into).collect(),
            commit_count: annotation.as_ref().map_or(0, |value| value.commit_count),
            annotation,
            diff_stat: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BranchAnnotation {
    pub(crate) meta: CommitMeta,
    pub(crate) commit_count: usize,
}

impl BranchAnnotation {
    pub(crate) const fn new(meta: CommitMeta, commit_count: usize) -> Self {
        Self { meta, commit_count }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BranchPointRef {
    pub(crate) oid: String,
    pub(crate) names: Vec<String>,
}

impl BranchPointRef {
    pub(crate) fn new<I, S>(oid: impl Into<String>, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            oid: oid.into(),
            names: names.into_iter().map(Into::into).collect(),
        }
    }
}

pub(crate) fn display_short_oid(oid: &str) -> String {
    oid.chars().take(DISPLAY_OID_LEN).collect()
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RewrittenCommit {
    pub(crate) meta: CommitMeta,
    pub(crate) replacement: CommitMeta,
}

impl RewrittenCommit {
    pub(crate) const fn new(meta: CommitMeta, replacement: CommitMeta) -> Self {
        Self { meta, replacement }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Lane {
    pub(crate) head_oid: String,
    pub(crate) base_oid: Option<String>,
    pub(crate) branch_points: Vec<BranchPoint>,
    pub(crate) rewritten_commits: Vec<RewrittenCommit>,
    pub(crate) head_timestamp: i64,
    #[serde(skip)]
    pub(crate) contains_current: bool,
}

impl Lane {
    pub(crate) fn new_with_rewritten_commits(
        head_oid: impl Into<String>,
        base_oid: Option<String>,
        branch_points: Vec<BranchPoint>,
        rewritten_commits: Vec<RewrittenCommit>,
        head_timestamp: i64,
        contains_current: bool,
    ) -> Self {
        Self {
            head_oid: head_oid.into(),
            base_oid,
            branch_points,
            rewritten_commits,
            head_timestamp,
            contains_current,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LaneGroup {
    pub(crate) base_oid: Option<String>,
    pub(crate) base_meta: Option<CommitMeta>,
    pub(crate) main_distance: Option<usize>,
    pub(crate) lanes: Vec<Lane>,
}

impl LaneGroup {
    pub(crate) const fn new(
        base_oid: Option<String>,
        base_meta: Option<CommitMeta>,
        main_distance: Option<usize>,
        lanes: Vec<Lane>,
    ) -> Self {
        Self {
            base_oid,
            base_meta,
            main_distance,
            lanes,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RepositorySnapshot {
    pub(crate) current_branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) main_name: String,
}

impl RepositorySnapshot {
    pub(crate) fn new(
        current_branch: Option<String>,
        head: Option<String>,
        main_name: impl Into<String>,
    ) -> Self {
        Self {
            current_branch,
            head,
            main_name: main_name.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum BuiltLanes {
    Empty {
        main_oid: String,
        repository: RepositorySnapshot,
    },
    Populated {
        lanes: Vec<Lane>,
        main_oid: String,
        repository: RepositorySnapshot,
    },
}

impl BuiltLanes {
    pub(crate) fn empty(main_oid: impl Into<String>, repository: RepositorySnapshot) -> Self {
        Self::Empty {
            main_oid: main_oid.into(),
            repository,
        }
    }

    pub(crate) fn populated(
        lanes: Vec<Lane>,
        main_oid: impl Into<String>,
        repository: RepositorySnapshot,
    ) -> Self {
        Self::Populated {
            lanes,
            main_oid: main_oid.into(),
            repository,
        }
    }
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;

const DISPLAY_OID_LEN: usize = 7;

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchPoint {
    pub(crate) oid: String,
    pub(crate) names: Vec<String>,
    pub(crate) annotation: Option<BranchAnnotation>,
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
            annotation,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchAnnotation {
    pub(crate) meta: CommitMeta,
    pub(crate) commit_count: usize,
}

impl BranchAnnotation {
    pub(crate) fn new(meta: CommitMeta, commit_count: usize) -> Self {
        Self { meta, commit_count }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Lane {
    pub(crate) head_oid: String,
    pub(crate) base_oid: Option<String>,
    pub(crate) branch_points: Vec<BranchPoint>,
    pub(crate) head_timestamp: i64,
    pub(crate) contains_current: bool,
}

impl Lane {
    pub(crate) fn new(
        head_oid: impl Into<String>,
        base_oid: Option<String>,
        branch_points: Vec<BranchPoint>,
        head_timestamp: i64,
        contains_current: bool,
    ) -> Self {
        Self {
            head_oid: head_oid.into(),
            base_oid,
            branch_points,
            head_timestamp,
            contains_current,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LaneGroup {
    pub(crate) base_oid: Option<String>,
    pub(crate) base_meta: Option<CommitMeta>,
    pub(crate) main_distance: Option<usize>,
    pub(crate) lanes: Vec<Lane>,
}

impl LaneGroup {
    pub(crate) fn new(
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

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
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
mod tests {
    use super::*;

    #[test]
    fn display_short_oid_uses_fixed_seven_character_width() {
        assert_eq!(
            display_short_oid("309567f69abcdef0123456789abcdef01234567"),
            "309567f"
        );
        assert_eq!(display_short_oid("abc123"), "abc123");
    }

    #[test]
    fn commit_metadata_derives_display_oid() {
        let meta = CommitMeta::new("309567f69abcdef0123456789abcdef01234567", 123, "subject");

        assert_eq!(meta.short_oid, "309567f");
        assert_eq!(meta.timestamp, 123);
        assert_eq!(meta.subject, "subject");
    }

    #[test]
    fn constructors_preserve_model_fields_without_recomputing_at_call_sites() {
        let meta = CommitMeta::new("abcdef123456", 99, "topic");
        let annotation = BranchAnnotation::new(meta.clone(), 2);
        let point = BranchPoint::new("abcdef123456", ["feature"], Some(annotation.clone()));
        let lane = Lane::new(
            "abcdef123456",
            Some("base".to_string()),
            vec![point],
            99,
            true,
        );
        let group = LaneGroup::new(
            Some("base".to_string()),
            Some(meta),
            Some(1),
            vec![lane.clone()],
        );
        let repository = RepositorySnapshot::new(
            Some("feature".to_string()),
            Some("abcdef123456".to_string()),
            "main",
        );

        assert_eq!(annotation.commit_count, 2);
        assert_eq!(lane.head_oid, "abcdef123456");
        assert_eq!(group.lanes, vec![lane]);
        assert_eq!(repository.main_name, "main");
    }
}

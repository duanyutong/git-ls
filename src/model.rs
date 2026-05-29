const DISPLAY_OID_LEN: usize = 7;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitMeta {
    pub(crate) oid: String,
    pub(crate) short_oid: String,
    pub(crate) subject: String,
    pub(crate) timestamp: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchPoint {
    pub(crate) oid: String,
    pub(crate) names: Vec<String>,
    pub(crate) annotation: Option<BranchAnnotation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchAnnotation {
    pub(crate) meta: CommitMeta,
    pub(crate) commit_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchPointRef {
    pub(crate) oid: String,
    pub(crate) names: Vec<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LaneGroup {
    pub(crate) base_oid: Option<String>,
    pub(crate) base_meta: Option<CommitMeta>,
    pub(crate) main_distance: Option<usize>,
    pub(crate) lanes: Vec<Lane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepositorySnapshot {
    pub(crate) current_branch: Option<String>,
    pub(crate) head: Option<String>,
    pub(crate) main_name: String,
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
}

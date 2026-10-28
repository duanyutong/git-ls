use crate::cli::{Layout, Verbosity};
use crate::model::{CommitMeta, LaneGroup, MainReference, RepositorySnapshot};

use super::ancestry::{PreparedAncestry, PreparedTopology};
use super::colours::Colours;
use super::diff::DiffStatWidths;
use super::metadata::MetadataWidths;

pub(crate) struct RenderContext<'a> {
    pub(super) main_name: &'a str,
    pub(super) main_meta: Option<&'a CommitMeta>,
    pub(super) current_branch: Option<&'a str>,
    pub(super) head: Option<&'a str>,
    pub(super) now_timestamp: i64,
    pub(super) verbosity: Verbosity,
    pub(super) metadata_widths: MetadataWidths,
    pub(super) layout: Layout,
    pub(super) colours: &'a Colours,
    pub(super) diff_stat_widths: DiffStatWidths,
    pub(super) topology: Option<&'a PreparedTopology>,
    pub(super) ancestry: Option<&'a PreparedAncestry>,
    pub(super) terminal_width: Option<usize>,
    pub(super) main_refs: &'a [MainReference],
    pub(super) main_oid: Option<&'a str>,
    pub(super) main_history_name: Option<&'a str>,
}

impl<'a> RenderContext<'a> {
    pub(crate) fn new(
        main_name: &'a str,
        main_meta: Option<&'a CommitMeta>,
        current_branch: Option<&'a str>,
        head: Option<&'a str>,
        now_timestamp: i64,
        verbosity: Verbosity,
        metadata_widths: MetadataWidths,
        colours: &'a Colours,
    ) -> Self {
        Self {
            main_name,
            main_meta,
            current_branch,
            head,
            now_timestamp,
            verbosity,
            metadata_widths,
            layout: Layout::default(),
            colours,
            diff_stat_widths: DiffStatWidths::default(),
            topology: None,
            ancestry: None,
            terminal_width: None,
            main_refs: &[],
            main_oid: None,
            main_history_name: None,
        }
    }

    pub(crate) const fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    pub(crate) fn with_mainline(mut self, repository: &'a RepositorySnapshot) -> Self {
        self.main_refs = &repository.main_refs;
        self.main_oid = repository.main_oid.as_deref();
        self
    }

    pub(crate) fn with_diff_stats(mut self, groups: &[LaneGroup]) -> Self {
        self.diff_stat_widths = DiffStatWidths::from_groups(groups);
        self
    }

    pub(crate) const fn with_topology(
        mut self,
        topology: &'a PreparedTopology,
        width: Option<usize>,
    ) -> Self {
        self.topology = Some(topology);
        self.terminal_width = width;
        self
    }
}

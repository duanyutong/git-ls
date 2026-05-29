use crate::cli::Verbosity;
use crate::model::CommitMeta;

use super::colours::Colours;
use super::metadata::MetadataWidths;

pub(crate) struct RenderContext<'a> {
    pub(super) main_name: &'a str,
    pub(super) main_meta: Option<&'a CommitMeta>,
    pub(super) current_branch: Option<&'a str>,
    pub(super) head: Option<&'a str>,
    pub(super) now_timestamp: i64,
    pub(super) verbosity: Verbosity,
    pub(super) metadata_widths: MetadataWidths,
    pub(super) colours: &'a Colours,
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
            colours,
        }
    }
}

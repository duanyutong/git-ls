use crate::cli::Verbosity;
use crate::model::CommitMeta;

use super::colours::Colours;
use super::metadata::MetadataWidths;

pub(crate) struct RenderContext<'a> {
    pub(crate) main_name: &'a str,
    pub(crate) main_meta: Option<&'a CommitMeta>,
    pub(crate) current_branch: Option<&'a str>,
    pub(crate) head: Option<&'a str>,
    pub(crate) now_timestamp: i64,
    pub(crate) verbosity: Verbosity,
    pub(crate) metadata_widths: MetadataWidths,
    pub(crate) colours: &'a Colours,
}

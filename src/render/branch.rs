use crate::cli::Verbosity;
use crate::model::BranchPoint;

use super::colours::Colours;
use super::metadata::{MetadataWidths, branch_metadata_columns, format_metadata_prefix};

pub(super) fn display_names(
    point: &BranchPoint,
    current_branch: Option<&str>,
    colour_index: usize,
    now_timestamp: i64,
    verbosity: Verbosity,
    metadata_widths: MetadataWidths,
    colours: &Colours,
) -> String {
    let names = point
        .names
        .iter()
        .map(|name| {
            if current_branch.is_some_and(|branch| branch == name) {
                colours.current_stack(colour_index, name)
            } else {
                colours.stack(colour_index, name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let Some(annotation) = point
        .annotation
        .as_ref()
        .filter(|_| verbosity.includes_metadata())
    else {
        return names;
    };

    let (age, count) = branch_metadata_columns(annotation, now_timestamp);
    let prefix = format_metadata_prefix(
        &age,
        &count,
        &annotation.meta.short_oid,
        metadata_widths,
        colours,
    );
    if verbosity.includes_title() {
        format!(
            "{prefix} {names} {}",
            colours.commit_title(&annotation.meta.subject)
        )
    } else {
        format!("{prefix} {names}")
    }
}

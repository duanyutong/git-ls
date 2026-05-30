use crate::cli::{Layout, Verbosity};
use crate::model::BranchPoint;

use super::colours::Colours;
use super::metadata::{
    MetadataWidths, branch_metadata_columns, columns_count, format_metadata_prefix,
};

pub(super) fn display_names(
    point: &BranchPoint,
    current_branch: Option<&str>,
    colour_index: usize,
    now_timestamp: i64,
    verbosity: Verbosity,
    metadata_widths: MetadataWidths,
    layout: Layout,
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

    if layout.is_columns() {
        let count_column = columns_count(&count, metadata_widths, colours, false);
        let mut label = format!("{count_column} {names}");
        if verbosity.includes_title() {
            label.push(' ');
            label.push_str(&colours.commit_title(&annotation.meta.subject));
        }
        if verbosity.includes_oid() {
            label.push(' ');
            label.push_str(&colours.metadata_oid(&annotation.meta.short_oid));
        }
        return label;
    }

    let prefix = format_metadata_prefix(
        &age,
        &count,
        verbosity
            .includes_oid()
            .then_some(annotation.meta.short_oid.as_str()),
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

mod ancestry;
mod branch;
mod colours;
mod context;
mod diff;
mod graph;
mod layout;
mod line;
mod metadata;
mod orphan;
mod rewrite;
mod trunk;

pub(crate) use ancestry::PreparedTopology;
pub(crate) use colours::Colours;
pub(crate) use context::RenderContext;
pub(crate) use diff::fit_diff_stats;
pub(crate) use layout::{render_empty_selection, render_lane_groups};
pub(crate) use line::RenderLine;
pub(crate) use metadata::calculate_metadata_widths;

#[cfg(test)]
mod tests;

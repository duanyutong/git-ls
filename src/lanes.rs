//! Branch selection, lane construction, and grouping policy.

mod construction;
mod diff;
mod grouping;
mod observer;
mod selection;

pub(crate) use construction::build_lanes;
pub(crate) use diff::populate_diff_stats;
pub(crate) use grouping::{build_lane_groups, ordered_lanes, sort_lane_groups};
pub(crate) use observer::AnalysisObserver;
#[cfg(test)]
pub(crate) use observer::AnalysisStage;
pub(crate) use selection::plain_git_main_branch;

#[cfg(test)]
use construction::*;
#[cfg(test)]
use grouping::*;
#[cfg(test)]
use selection::*;
#[cfg(test)]
mod tests;

mod batch;
mod diff;
mod gix;
mod graph;
mod metadata;
mod process;
mod session;
mod shell;
mod traits;

pub(crate) use diff::{DiffBatch, DiffPair};
pub(crate) use gix::GixBackend;
pub(crate) use metadata::get_commit_meta;
pub(crate) use process::ProcessGit;
pub(crate) use session::{Inputs, Session};
#[cfg(test)]
pub(crate) use shell::non_empty;
pub(crate) use traits::{
    AncestryBackend, BranchlessQueries, CommitMetadataBackend, DiffStatBackend, GitBackend,
    GitCommand, RepositoryStateBackend,
};

#[cfg(test)]
mod tests;

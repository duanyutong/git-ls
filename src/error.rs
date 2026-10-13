use std::fmt;
use std::io;
use std::num::ParseIntError;

use thiserror::Error;

/// Error type returned by the public `git-ls` entry points.
///
/// Display text is suitable for command-line diagnostics. Variant fields retain
/// structured implementation context for callers that need to inspect failures
/// without parsing rendered messages.
#[derive(Debug, Error)]
pub enum GitLsError {
    /// Command-line argument parsing failed.
    #[error(transparent)]
    Cli(#[from] clap::Error),

    /// The `git` executable could not be started.
    #[error("failed to execute git: {0}")]
    GitExec(#[source] io::Error),

    /// A `git` subprocess completed unsuccessfully.
    #[error("git {args} failed: {detail}")]
    GitCommand { args: String, detail: String },

    /// A `gix` operation failed.
    #[error("gix {context} failed: {detail}")]
    Gix {
        context: &'static str,
        detail: String,
    },

    /// A value expected to be a Git object identifier was malformed.
    #[error("invalid git object id {oid}: {detail}")]
    InvalidObjectId { oid: String, detail: String },

    /// `git show` returned a record that did not match the requested object set.
    #[error("unexpected git show output for {oid}")]
    UnexpectedGitShow { oid: String },

    /// A commit timestamp was present but not parseable as an integer.
    #[error("invalid commit timestamp for {oid}: {source}")]
    InvalidCommitTimestamp {
        oid: String,
        #[source]
        source: ParseIntError,
    },

    /// The configured main-branch revset did not resolve to exactly one commit.
    #[error("expected main() to resolve to one commit, got {count}")]
    AmbiguousMainRevset { count: usize },

    /// Plain Git fallback could not identify the repository's main branch.
    #[error("could not resolve plain Git main branch from local branches; tried {candidates}")]
    PlainGitMainBranchNotFound { candidates: String },

    /// A `git config` value was present but outside the accepted value domain.
    #[error("invalid git config {key}={value:?}: expected {expected}")]
    InvalidGitConfig {
        key: &'static str,
        value: String,
        expected: &'static str,
    },

    #[error("failed to write output: {0}")]
    Write(#[from] io::Error),

    #[cfg(test)]
    #[error("{0}")]
    TestFixture(String),
}

impl GitLsError {
    pub(crate) fn git_command(args: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::GitCommand {
            args: args.into(),
            detail: detail.into(),
        }
    }

    pub(crate) fn gix(context: &'static str, source: impl fmt::Display) -> Self {
        Self::Gix {
            context,
            detail: source.to_string(),
        }
    }

    pub(crate) fn invalid_object_id(oid: impl Into<String>, source: impl fmt::Display) -> Self {
        Self::InvalidObjectId {
            oid: oid.into(),
            detail: source.to_string(),
        }
    }

    pub(crate) fn unexpected_git_show(oid: impl Into<String>) -> Self {
        Self::UnexpectedGitShow { oid: oid.into() }
    }

    pub(crate) fn invalid_commit_timestamp(oid: impl Into<String>, source: ParseIntError) -> Self {
        Self::InvalidCommitTimestamp {
            oid: oid.into(),
            source,
        }
    }

    pub(crate) const fn ambiguous_main_revset(count: usize) -> Self {
        Self::AmbiguousMainRevset { count }
    }

    pub(crate) fn plain_git_main_branch_not_found(candidates: impl Into<String>) -> Self {
        Self::PlainGitMainBranchNotFound {
            candidates: candidates.into(),
        }
    }

    pub(crate) fn invalid_git_config(
        key: &'static str,
        value: impl Into<String>,
        expected: &'static str,
    ) -> Self {
        Self::InvalidGitConfig {
            key,
            value: value.into(),
            expected,
        }
    }
}

pub type Result<T> = std::result::Result<T, GitLsError>;

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;

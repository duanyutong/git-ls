use std::io;
use std::num::ParseIntError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitLsError {
    #[error(transparent)]
    Cli(#[from] clap::Error),

    #[error("failed to execute git: {0}")]
    GitExec(#[source] io::Error),

    #[error("git {args} failed: {detail}")]
    GitCommand { args: String, detail: String },

    #[error("gix {context} failed: {detail}")]
    Gix {
        context: &'static str,
        detail: String,
    },

    #[error("invalid git object id {oid}: {detail}")]
    InvalidObjectId { oid: String, detail: String },

    #[error("unexpected git show output for {oid}")]
    UnexpectedGitShow { oid: String },

    #[error("invalid commit timestamp for {oid}: {source}")]
    InvalidCommitTimestamp {
        oid: String,
        #[source]
        source: ParseIntError,
    },

    #[error("expected main() to resolve to one commit, got {count}")]
    AmbiguousMainRevset { count: usize },

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

pub type Result<T> = std::result::Result<T, GitLsError>;

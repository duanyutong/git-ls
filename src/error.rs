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

    pub(crate) fn ambiguous_main_revset(count: usize) -> Self {
        Self::AmbiguousMainRevset { count }
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
mod tests {
    use std::error::Error as _;
    use std::io;

    use clap::error::ErrorKind;

    use super::*;

    fn assert_same_variant(actual: &GitLsError, expected: &GitLsError) {
        assert_eq!(
            std::mem::discriminant(actual),
            std::mem::discriminant(expected)
        );
    }

    #[test]
    fn display_text_names_error_context_without_exposing_variant_syntax() {
        let timestamp_source = "not-a-timestamp".parse::<i64>().unwrap_err();
        let cases = vec![
            (
                GitLsError::git_command("status --short", "fatal: not a git repository"),
                "git status --short failed: fatal: not a git repository",
            ),
            (
                GitLsError::gix("discover repository", "no repository found"),
                "gix discover repository failed: no repository found",
            ),
            (
                GitLsError::invalid_object_id("abc", "invalid length"),
                "invalid git object id abc: invalid length",
            ),
            (
                GitLsError::unexpected_git_show("abc123"),
                "unexpected git show output for abc123",
            ),
            (
                GitLsError::invalid_commit_timestamp("abc123", timestamp_source),
                "invalid commit timestamp for abc123: invalid digit found in string",
            ),
            (
                GitLsError::ambiguous_main_revset(2),
                "expected main() to resolve to one commit, got 2",
            ),
            (
                GitLsError::invalid_git_config("git-ls.backend", "svn", "gix or shell"),
                "invalid git config git-ls.backend=\"svn\": expected gix or shell",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn source_chain_is_preserved_for_wrapped_errors() {
        let write_error: GitLsError =
            io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed").into();
        assert_same_variant(
            &write_error,
            &GitLsError::Write(io::Error::other("variant marker")),
        );
        assert_eq!(write_error.source().unwrap().to_string(), "pipe closed");

        let exec_error =
            GitLsError::GitExec(io::Error::new(io::ErrorKind::NotFound, "git missing"));
        assert_eq!(exec_error.source().unwrap().to_string(), "git missing");

        let timestamp_source = "soon".parse::<i64>().unwrap_err();
        let timestamp_error = GitLsError::invalid_commit_timestamp("abc123", timestamp_source);
        assert_eq!(
            timestamp_error.source().unwrap().to_string(),
            "invalid digit found in string"
        );
    }

    #[test]
    fn textual_backend_diagnostics_do_not_create_source_chain() {
        let command_failure = GitLsError::git_command("status", "fatal: no work tree");
        let repository_failure = GitLsError::gix("find merge base", "graph traversal failed");

        assert!(command_failure.source().is_none());
        assert!(repository_failure.source().is_none());
    }

    #[test]
    fn from_conversions_route_to_boundary_variants() {
        let cli_error: GitLsError =
            clap::Error::raw(ErrorKind::UnknownArgument, "unexpected argument").into();
        assert_same_variant(
            &cli_error,
            &GitLsError::Cli(clap::Error::raw(
                ErrorKind::UnknownArgument,
                "variant marker",
            )),
        );

        let write_error: GitLsError =
            io::Error::new(io::ErrorKind::Interrupted, "write interrupted").into();
        assert_same_variant(
            &write_error,
            &GitLsError::Write(io::Error::other("variant marker")),
        );
    }

    #[test]
    fn constructors_preserve_structured_diagnostic_fields() {
        let error = GitLsError::invalid_git_config("git-ls.palette", "mauve", "classic");
        assert!(matches!(
            error,
            GitLsError::InvalidGitConfig {
                key: "git-ls.palette",
                value,
                expected: "classic",
            } if value == "mauve"
        ));

        let error = GitLsError::gix("read HEAD name", "invalid ref");
        assert!(matches!(
            error,
            GitLsError::Gix {
                context: "read HEAD name",
                detail,
            } if detail == "invalid ref"
        ));
    }
}

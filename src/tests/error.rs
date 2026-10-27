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
            GitLsError::plain_git_main_branch_not_found("main, master"),
            "could not resolve plain Git main branch from local branches; tried main, master",
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
    let write_error: GitLsError = io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed").into();
    assert_same_variant(
        &write_error,
        &GitLsError::Write(io::Error::other("variant marker")),
    );
    assert_eq!(write_error.source().unwrap().to_string(), "pipe closed");

    let exec_error = GitLsError::GitExec(io::Error::new(io::ErrorKind::NotFound, "git missing"));
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
    let error = GitLsError::invalid_git_config("git-ls.layout", "wide", "columns or inline");
    assert!(matches!(
        error,
        GitLsError::InvalidGitConfig {
            key: "git-ls.layout",
            value,
            expected: "columns or inline",
        } if value == "wide"
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

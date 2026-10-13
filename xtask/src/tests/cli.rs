use clap::Parser;

use super::*;

#[test]
fn parses_amend_arguments() {
    let cli = Cli::try_parse_from(["xtask", "version", "amend", "--no-edit"]).unwrap();
    assert_eq!(
        cli.command,
        Commands::Version(VersionCommand::Amend {
            args: vec!["--no-edit".into()]
        })
    );
}

#[test]
fn parses_commit_msg_command() {
    let cli =
        Cli::try_parse_from(["xtask", "version", "commit-msg", ".git/COMMIT_EDITMSG"]).unwrap();

    assert_eq!(
        cli.command,
        Commands::Version(VersionCommand::CommitMsg {
            message_file: ".git/COMMIT_EDITMSG".to_string(),
        })
    );
}

#[test]
fn parses_verify_range_command() {
    let cli = Cli::try_parse_from(["xtask", "version", "verify-range", "v0.1.0", "HEAD"]).unwrap();

    assert_eq!(
        cli.command,
        Commands::Version(VersionCommand::VerifyRange {
            base: "v0.1.0".to_string(),
            head: "HEAD".to_string(),
        })
    );
}

#[test]
fn parses_tag_range_command() {
    let cli = Cli::try_parse_from(["xtask", "version", "tag-range", "v0.1.0", "HEAD"]).unwrap();

    assert_eq!(
        cli.command,
        Commands::Version(VersionCommand::TagRange {
            base: "v0.1.0".to_string(),
            head: "HEAD".to_string(),
        })
    );
}

#[test]
fn parses_pre_push_command() {
    let cli = Cli::try_parse_from(["xtask", "version", "pre-push"]).unwrap();

    assert_eq!(cli.command, Commands::Version(VersionCommand::PrePush));
}

#[test]
fn rejects_missing_version_subcommand() {
    let error = Cli::try_parse_from(["xtask", "version"]).unwrap_err();

    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
}

use clap::{Parser, Subcommand};

#[derive(Debug, Eq, Parser, PartialEq)]
#[command(name = "xtask", about = "Repository maintenance tasks.")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Eq, PartialEq, Subcommand)]
pub(crate) enum Commands {
    #[command(subcommand)]
    Version(VersionCommand),
}

#[derive(Debug, Eq, PartialEq, Subcommand)]
pub(crate) enum VersionCommand {
    /// Check the pending commit message and generate an unstaged version bump if required.
    CommitMsg { message_file: String },

    /// Verify every commit in a Git range contains the expected version bump.
    VerifyRange { base: String, head: String },

    /// Verify and tag every commit in a Git range by its Cargo package version.
    TagRange { base: String, head: String },

    /// Verify commits that are being pushed, using Git's pre-push hook input.
    PrePush,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

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
        let cli =
            Cli::try_parse_from(["xtask", "version", "verify-range", "v0.1.0", "HEAD"]).unwrap();

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
}

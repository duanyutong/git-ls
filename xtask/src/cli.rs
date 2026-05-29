use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repository maintenance tasks.")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    #[command(subcommand)]
    Version(VersionCommand),
}

#[derive(Debug, Subcommand)]
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

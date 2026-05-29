use clap::Parser;

mod cli;
mod git;
mod manifest;
mod policy;
mod version;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    match cli.command {
        cli::Commands::Version(cli::VersionCommand::CommitMsg { message_file }) => {
            version::commit_msg(std::path::Path::new(&message_file))?;
        }
        cli::Commands::Version(cli::VersionCommand::VerifyRange { base, head }) => {
            version::verify_range(&base, &head)?;
        }
        cli::Commands::Version(cli::VersionCommand::TagRange { base, head }) => {
            version::tag_range(&base, &head)?;
        }
        cli::Commands::Version(cli::VersionCommand::PrePush) => {
            version::pre_push()?;
        }
    }

    Ok(())
}

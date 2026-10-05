use std::process::{Command, Output};

use crate::error::{GitLsError, Result};

use super::traits::GitCommand;

#[derive(Debug, Default)]
pub(crate) struct ProcessGit;

impl GitCommand for ProcessGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let family = match args.first().copied() {
            Some("config") => "subprocess_config",
            Some("merge-base") => "subprocess_merge_base",
            Some("rev-list") => "subprocess_rev_list",
            Some("show") => "subprocess_metadata",
            Some("branchless") => "subprocess_branchless",
            _ => "subprocess_repository",
        };
        let _perf = crate::diagnostics::operation(family);
        crate::diagnostics::count("subprocess_calls", 1);
        let output = execute_git(args)?;

        if !output.status.success() && (!allow_failure || output.status.code() != Some(1)) {
            return Err(GitLsError::git_command(
                args.join(" "),
                command_failure_detail(&output),
            ));
        }

        Ok(normalised_stdout(&output))
    }
    fn run_input(&self, args: &[&str], input: &str) -> Result<String> {
        use std::io::{Seek, Write};
        let _perf = crate::diagnostics::operation("subprocess_rev_list");
        crate::diagnostics::count("subprocess_calls", 1);
        let mut spool = tempfile::tempfile().map_err(GitLsError::GitExec)?;
        spool
            .write_all(input.as_bytes())
            .map_err(GitLsError::GitExec)?;
        spool.rewind().map_err(GitLsError::GitExec)?;
        let output = Command::new("git")
            .args(args)
            .stdin(spool)
            .output()
            .map_err(GitLsError::GitExec)?;
        if !output.status.success() {
            return Err(GitLsError::git_command(
                args.join(" "),
                command_failure_detail(&output),
            ));
        }
        Ok(normalised_stdout(&output))
    }
}

fn execute_git(args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .output()
        .map_err(GitLsError::GitExec)
}

fn normalised_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string()
}

fn command_failure_detail(output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        detail
    }
}

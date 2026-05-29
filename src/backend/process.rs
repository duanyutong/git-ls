use std::process::{Command, Output};

use crate::error::{GitLsError, Result};

use super::traits::GitCommand;

#[derive(Debug, Default)]
pub(crate) struct ProcessGit;

impl GitCommand for ProcessGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let output = execute_git(args)?;

        if !output.status.success() && !allow_failure {
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

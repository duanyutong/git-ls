use std::cell::{Cell, RefCell};
use std::process::{Command, Output, Stdio};

use crate::error::{GitLsError, Result};

use super::traits::GitCommand;

#[derive(Debug, Default)]
pub(crate) struct ProcessGit {
    config: RefCell<Option<String>>,
    context_captured: Cell<bool>,
    empty_tree: RefCell<Option<String>>,
}

impl GitCommand for ProcessGit {
    fn empty_tree(&self) -> Result<String> {
        if let Some(tree) = self.empty_tree.borrow().as_ref() {
            return Ok(tree.clone());
        }
        let tree = self.run_input(&["hash-object", "-t", "tree", "--stdin"], "")?;
        self.empty_tree.replace(Some(tree.clone()));
        Ok(tree)
    }

    fn config_values(&self, _keys: &[&str]) -> Result<std::collections::HashMap<String, String>> {
        let output = self.run(&["config", "--null", "--list"], false)?;
        let mut values = super::shell::parse_config_records(&output);
        // Git performs Boolean conversion collectively, including valueless keys.
        let keys = ["git-ls.diffstat", "git-ls.cache", "git-ls.cachemaintenance"];
        if keys.iter().any(|key| values.contains_key(*key)) {
            let booleans = self.run(
                &[
                    "config",
                    "--null",
                    "--type=bool",
                    "--get-regexp",
                    "^git-ls\\.(diffstat|cache|cachemaintenance)$",
                ],
                false,
            )?;
            values.extend(super::shell::parse_config_records(&booleans));
        }
        self.config.replace(Some(output));
        Ok(values)
    }

    fn boolean_values(
        &self,
        values: &std::collections::HashMap<String, String>,
    ) -> Result<std::collections::HashMap<String, String>> {
        // config_values has already delegated these conversions to Git.
        Ok(values.clone())
    }

    fn diff_context(&self, paths: &[String]) -> Option<u64> {
        let _perf = crate::diagnostics::stage("diff_context");
        if paths.iter().any(|path| path.contains('\u{fffd}')) {
            return None;
        }
        let repo = gix::discover_with_environment_overrides(".").ok()?;
        let root = repo.workdir()?.canonicalize().ok()?;
        let config = if self.context_captured.replace(true) || self.config.borrow().is_none() {
            self.run(&["config", "--null", "--list"], false).ok()?
        } else {
            self.config.borrow().as_ref()?.clone()
        };
        if config.contains('\u{fffd}') {
            return None;
        }
        let mut input = String::new();
        for path in paths {
            input.push_str(path);
            input.push('\0');
        }
        let attributes = if paths.is_empty() {
            String::new()
        } else {
            self.run_input(
                &["-C", root.to_str()?, "check-attr", "--all", "-z", "--stdin"],
                &input,
            )
            .ok()?
        };
        let executable = std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|directory| directory.join(format!("git{}", std::env::consts::EXE_SUFFIX)))
            .find(|path| path.is_file())?;
        let metadata = executable.metadata().ok()?;
        Some(crate::cache::hash(&(
            "diff-v1",
            root,
            config,
            attributes,
            executable.canonicalize().ok()?,
            metadata.len(),
            metadata.modified().ok()?,
        )))
    }

    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let _perf = crate::diagnostics::operation(subprocess_family(args));
        crate::diagnostics::count("subprocess_calls", 1);
        let output = execute_git(args)?;

        if !output.status.success() && (!allow_failure || output.status.code() != Some(1)) {
            return Err(GitLsError::git_command(
                args.join(" "),
                command_failure_detail(&output),
            ));
        }

        let mut text = decoded_stdout(output);
        text.truncate(text.trim_end_matches('\n').len());
        Ok(text)
    }
    fn run_input(&self, args: &[&str], input: &str) -> Result<String> {
        use std::io::{Seek, Write};
        let _perf = crate::diagnostics::operation(subprocess_family(args));
        crate::diagnostics::count("subprocess_calls", 1);
        let stdin = if input.is_empty() {
            Stdio::null()
        } else {
            let mut spool = tempfile::tempfile().map_err(GitLsError::GitExec)?;
            spool
                .write_all(input.as_bytes())
                .map_err(GitLsError::GitExec)?;
            spool.rewind().map_err(GitLsError::GitExec)?;
            Stdio::from(spool)
        };
        let output = Command::new("git")
            .args(args)
            .stdin(stdin)
            .output()
            .map_err(GitLsError::GitExec)?;
        if !output.status.success() {
            return Err(GitLsError::git_command(
                args.join(" "),
                command_failure_detail(&output),
            ));
        }
        // Preserve framing, including a final header with no numstat records.
        Ok(decoded_stdout(output))
    }
}

fn execute_git(args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .output()
        .map_err(GitLsError::GitExec)
}

fn subprocess_family(args: &[&str]) -> &'static str {
    let command = if args.first() == Some(&"-C") {
        args.get(2)
    } else {
        args.first()
    };
    match command.copied() {
        Some("config") => "subprocess_config",
        Some("merge-base") => "subprocess_merge_base",
        Some("rev-list") => "subprocess_rev_list",
        Some("show" | "cat-file") => "subprocess_metadata",
        Some("branchless") => "subprocess_branchless",
        Some("diff" | "diff-tree") => "subprocess_diff",
        Some("hash-object") => "subprocess_hash_object",
        Some("check-attr") => "subprocess_attributes",
        _ => "subprocess_repository",
    }
}

fn decoded_stdout(output: Output) -> String {
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

fn command_failure_detail(output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        detail
    }
}

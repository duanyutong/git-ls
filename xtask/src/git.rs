use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::process::Command;

use crate::Result;

pub(crate) const ZERO_OID: &str = "0000000000000000000000000000000000000000";

pub(crate) fn commits_in_range(base: &str, head: &str) -> Result<Vec<String>> {
    let base = if base == ZERO_OID {
        latest_reachable_version_tag(head)?
            .ok_or_else(|| format!("no reachable v* tag found for {head}"))?
    } else {
        base.to_string()
    };

    if base == head {
        return Ok(Vec::new());
    }

    let output = git_output([
        "rev-list",
        "--reverse",
        "--ancestry-path",
        &format!("{base}..{head}"),
    ])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub(crate) fn parent_count(commit: &str) -> Result<usize> {
    let output = git_output(["rev-list", "--parents", "-n", "1", commit])?;
    Ok(output.split_whitespace().skip(1).count())
}

pub(crate) fn latest_reachable_version_tag(commit: &str) -> Result<Option<String>> {
    let output = git_output_allow_failure([
        "describe",
        "--tags",
        "--abbrev=0",
        "--match",
        "v[0-9]*",
        commit,
    ])?;
    Ok(output
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub(crate) fn existing_tags() -> Result<BTreeMap<String, String>> {
    let output = git_output([
        "for-each-ref",
        "refs/tags",
        "--format=%(refname:short) %(objectname) %(*objectname)",
    ])?;
    let mut tags = BTreeMap::new();

    for line in output.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        let Some(tag) = parts.first() else {
            continue;
        };
        let Some(commit) = parts.last() else {
            continue;
        };
        if tag.starts_with('v') {
            tags.insert(tag.to_string(), commit.to_string());
        }
    }

    Ok(tags)
}

pub(crate) fn git_output<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git_output_allow_failure(args)?.ok_or_else(|| "git command failed".into())
}

fn git_output_allow_failure<I, S>(args: I) -> Result<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string(),
    ))
}

pub(crate) fn run_git<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_command(Command::new("git").args(args))
}

pub(crate) fn run_command(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    Err(detail.into())
}

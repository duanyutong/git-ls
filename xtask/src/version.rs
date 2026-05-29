use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use semver::Version;
use toml_edit::{DocumentMut, value};

use crate::Result;
use crate::git::{
    ZERO_OID, commits_in_range, existing_tags, git_output, latest_reachable_version_tag,
    parent_count, run_command, run_git,
};
use crate::manifest::{CARGO_LOCK, CARGO_TOML, cargo_lock_version, cargo_version_from_toml};
use crate::policy::{bumped, policy_from_message};

pub(crate) fn commit_msg(message_file: &Path) -> Result<()> {
    let message = fs::read_to_string(message_file)?;
    let policy = policy_from_message(&message)?;
    let head_version = cargo_version_at_ref("HEAD")?;
    let expected = bumped(&head_version, policy.bump);
    let staged = cargo_version_at_ref(":")?;

    if staged == expected {
        return Ok(());
    }

    if staged != head_version {
        return Err(format!(
            "staged Cargo package version is {staged}, but `{}` requires {expected} from {head_version}",
            policy.subject
        )
        .into());
    }

    update_worktree_version(&expected)?;
    eprintln!(
        "Version bump required: {head_version} -> {expected} for `{}`.",
        policy.subject
    );
    eprintln!("Updated {CARGO_TOML} and {CARGO_LOCK}; review, stage them, and re-run git commit.");

    Err("commit stopped after generating the required version bump".into())
}

pub(crate) fn verify_range(base: &str, head: &str) -> Result<()> {
    for commit in commits_in_range(base, head)? {
        verify_commit(&commit)?;
    }
    Ok(())
}

pub(crate) fn tag_range(base: &str, head: &str) -> Result<()> {
    let commits = commits_in_range(base, head)?;
    let existing = existing_tags()?;
    let mut tags_to_push = Vec::new();

    for commit in commits {
        verify_commit(&commit)?;
        let version = cargo_version_at_commit(&commit)?;
        let tag = format!("v{version}");

        if let Some(tagged_commit) = existing.get(&tag) {
            if tagged_commit == &commit {
                continue;
            }
            return Err(
                format!("tag {tag} already points at {tagged_commit}, not {commit}").into(),
            );
        }

        run_git(["tag", "-a", &tag, &commit, "-m", tag.as_str()])?;
        tags_to_push.push(tag);
    }

    if !tags_to_push.is_empty() {
        let mut args = vec!["push", "origin"];
        args.extend(tags_to_push.iter().map(String::as_str));
        run_git(args)?;
    }

    Ok(())
}

pub(crate) fn pre_push() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 4 {
            return Err(format!("invalid pre-push input line: {line}").into());
        }

        let local_ref = fields[0];
        let local_oid = fields[1];
        let remote_oid = fields[3];

        if local_oid == ZERO_OID {
            continue;
        }

        let base = if remote_oid == ZERO_OID {
            latest_reachable_version_tag(local_oid)?
                .ok_or_else(|| format!("no reachable v* tag found for {local_ref}"))?
        } else {
            remote_oid.to_string()
        };

        verify_range(&base, local_oid)?;
    }

    Ok(())
}

fn verify_commit(commit: &str) -> Result<()> {
    if parent_count(commit)? != 1 {
        return Err(format!("commit {commit} is not a single-parent commit").into());
    }

    let parent = git_output(["rev-parse", &format!("{commit}^")])?;
    let message = git_output(["log", "-1", "--format=%B", commit])?;
    let policy = policy_from_message(&message)?;
    let parent_version = cargo_version_at_commit(&parent)?;
    let actual = cargo_version_at_commit(commit)?;
    let expected = bumped(&parent_version, policy.bump);

    if actual != expected {
        return Err(format!(
            "commit {commit} `{}` has Cargo version {actual}; expected {expected} from parent {parent_version}",
            policy.subject
        )
        .into());
    }

    let lock_version = cargo_lock_version_at_commit(commit)?;
    if lock_version != actual {
        return Err(format!(
            "commit {commit} has {CARGO_TOML} version {actual}, but {CARGO_LOCK} records {lock_version}"
        )
        .into());
    }

    Ok(())
}

fn update_worktree_version(version: &Version) -> Result<()> {
    let cargo_toml = fs::read_to_string(CARGO_TOML)?;
    let mut document = cargo_toml.parse::<DocumentMut>()?;
    document["package"]["version"] = value(version.to_string());
    fs::write(CARGO_TOML, document.to_string())?;

    run_command(
        Command::new("cargo")
            .args(["update", "-w", "--offline"])
            .stdin(Stdio::null()),
    )?;
    Ok(())
}

fn cargo_version_at_ref(reference: &str) -> Result<Version> {
    let target = if reference == ":" {
        format!(":{CARGO_TOML}")
    } else {
        format!("{reference}:{CARGO_TOML}")
    };
    let cargo_toml = git_output(["show", target.as_str()])?;
    cargo_version_from_toml(&cargo_toml)
}

fn cargo_version_at_commit(commit: &str) -> Result<Version> {
    cargo_version_at_ref(commit)
}

fn cargo_lock_version_at_commit(commit: &str) -> Result<Version> {
    let cargo_lock = git_output(["show", &format!("{commit}:{CARGO_LOCK}")])?;
    cargo_lock_version(&cargo_lock)
}

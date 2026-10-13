use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

use semver::Version;
use toml_edit::{DocumentMut, value};

use crate::Result;
use crate::git::{
    ZERO_OID, commits_in_range, existing_tags, git_output, git_output_allow_failure, parent_count,
    run_git,
};
use crate::manifest::{CARGO_LOCK, CARGO_TOML, cargo_lock_version, cargo_version_from_toml};
use crate::policy::{bumped, policy_from_message};

pub(crate) fn commit_msg(message_file: &Path) -> Result<()> {
    let message = fs::read_to_string(message_file)?;
    let policy = policy_from_message(&message)?;
    let base = std::env::var("GIT_VERSION_BASE").unwrap_or_else(|_| "HEAD".into());
    let staged = cargo_version_at_ref(":")?;
    let staged_lock = cargo_lock_version(&git_output(["show", ":Cargo.lock"])?)?;
    if staged_lock != staged {
        return Err("staged Cargo.toml and Cargo.lock versions differ".into());
    }
    if base == "ROOT"
        || (base == "HEAD"
            && git_output_allow_failure(["rev-parse", "--verify", "HEAD"])?.is_none())
    {
        return Ok(());
    }
    let head_version = cargo_version_at_ref(&base)?;
    let expected = bumped(&head_version, policy.bump)?;

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

    if !git_output(["diff", "--", CARGO_TOML, CARGO_LOCK])?.is_empty() {
        return Err("stage or restore existing manifest and lockfile edits before generating a version bump".into());
    }
    update_worktree_version(&expected)?;
    eprintln!(
        "Version bump required: {head_version} -> {expected} for `{}`.",
        policy.subject
    );
    eprintln!("Updated {CARGO_TOML} and {CARGO_LOCK}; review, stage them, and re-run git commit.");

    Err("commit stopped after generating the required version bump".into())
}

pub(crate) fn amend(args: &[std::ffi::OsString]) -> Result<()> {
    let base = match parent_count("HEAD")? {
        0 => "ROOT".to_string(),
        1 => git_output(["rev-parse", "HEAD^"])?,
        _ => return Err("versioned commits must have at most one parent".into()),
    };
    let status = Command::new("git")
        .args(["commit", "--amend"])
        .args(args)
        .env("GIT_VERSION_BASE", base)
        .status()?;
    if !status.success() {
        return Err("git commit --amend failed".into());
    }
    Ok(())
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
    let mut tags_to_push = std::collections::BTreeMap::new();

    for commit in commits {
        verify_commit(&commit)?;
        let version = cargo_version_at_commit(&commit)?;
        let tag = format!("v{version}");

        if let Some(tagged_commit) = existing.get(&tag)
            && tagged_commit != &commit
        {
            return Err(
                format!("tag {tag} already points at {tagged_commit}, not {commit}").into(),
            );
        }
        if tags_to_push.insert(tag.clone(), commit).is_some() {
            return Err(format!("multiple commits claim tag {tag}").into());
        }
    }

    for (tag, commit) in &tags_to_push {
        if !existing.contains_key(tag) {
            run_git(["tag", "-a", tag, commit, "-m", tag])?;
        }
    }
    if !tags_to_push.is_empty() {
        let mut args = vec!["push".to_string(), "--atomic".into(), "origin".into()];
        args.extend(
            tags_to_push
                .keys()
                .map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}")),
        );
        run_git(args)?;
    }

    Ok(())
}

pub(crate) fn pre_push() -> Result<()> {
    // prek consumes Git's standard input and supplies the revisions as environment variables.
    if let Ok(head) = std::env::var("PRE_COMMIT_TO_REF") {
        let base = std::env::var("PRE_COMMIT_FROM_REF").unwrap_or_else(|_| ZERO_OID.into());
        return verify_range(&base, &head);
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 4 {
            return Err(format!("invalid pre-push input line: {line}").into());
        }

        let local_oid = fields[1];
        let remote_oid = fields[3];

        if local_oid == ZERO_OID {
            continue;
        }

        verify_range(remote_oid, local_oid)?;
    }

    Ok(())
}

fn verify_commit(commit: &str) -> Result<()> {
    let message = git_output(["log", "-1", "--format=%B", commit])?;
    let policy = policy_from_message(&message)?;
    let actual = cargo_version_at_commit(commit)?;
    match parent_count(commit)? {
        0 => {}
        1 => {
            let parent = git_output(["rev-parse", &format!("{commit}^")])?;
            let parent_version = cargo_version_at_commit(&parent)?;
            let expected = bumped(&parent_version, policy.bump)?;

            if actual != expected {
                return Err(format!(
                    "commit {commit} `{}` has Cargo version {actual}; expected {expected} from parent {parent_version}",
                    policy.subject
                )
                .into());
            }
        }
        _ => {
            return Err(
                format!("commit {commit} has multiple parents; rebase before release").into(),
            );
        }
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
    let package_name = document
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or("missing package name")?;
    let mut lock = fs::read_to_string(CARGO_LOCK)?.parse::<DocumentMut>()?;
    let packages = lock
        .get_mut("package")
        .and_then(|p| p.as_array_of_tables_mut())
        .ok_or("missing lockfile packages")?;
    let package = packages
        .iter_mut()
        .find(|package| package.get("name").and_then(|n| n.as_str()) == Some(package_name))
        .ok_or("package missing from lockfile")?;
    package["version"] = value(version.to_string());
    fs::write(CARGO_TOML, document.to_string())?;
    fs::write(CARGO_LOCK, lock.to_string())?;
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

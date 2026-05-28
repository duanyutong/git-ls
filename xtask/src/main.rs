use clap::{Parser, Subcommand};
use semver::Version;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use toml_edit::{DocumentMut, value};

const CARGO_TOML: &str = "Cargo.toml";
const CARGO_LOCK: &str = "Cargo.lock";
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repository maintenance tasks.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Version(VersionCommand),
}

#[derive(Debug, Subcommand)]
enum VersionCommand {
    /// Check the pending commit message and generate an unstaged version bump if required.
    CommitMsg { message_file: String },

    /// Verify every commit in a Git range contains the expected version bump.
    VerifyRange { base: String, head: String },

    /// Verify and tag every commit in a Git range by its Cargo package version.
    TagRange { base: String, head: String },

    /// Verify commits that are being pushed, using Git's pre-push hook input.
    PrePush,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommitPolicy {
    subject: String,
    bump: Bump,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version(VersionCommand::CommitMsg { message_file }) => {
            commit_msg(Path::new(&message_file))?;
        }
        Commands::Version(VersionCommand::VerifyRange { base, head }) => {
            verify_range(&base, &head)?;
        }
        Commands::Version(VersionCommand::TagRange { base, head }) => {
            tag_range(&base, &head)?;
        }
        Commands::Version(VersionCommand::PrePush) => {
            pre_push()?;
        }
    }

    Ok(())
}

fn commit_msg(message_file: &Path) -> Result<()> {
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

fn verify_range(base: &str, head: &str) -> Result<()> {
    for commit in commits_in_range(base, head)? {
        verify_commit(&commit)?;
    }
    Ok(())
}

fn tag_range(base: &str, head: &str) -> Result<()> {
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

fn pre_push() -> Result<()> {
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

fn policy_from_message(message: &str) -> Result<CommitPolicy> {
    let subject = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or("commit message is empty")?
        .to_string();

    let has_breaking_footer = message.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("BREAKING CHANGE:") || line.starts_with("BREAKING-CHANGE:")
    });

    if has_breaking_footer || header_is_breaking(&subject) {
        return Ok(CommitPolicy {
            subject,
            bump: Bump::Major,
        });
    }

    if conventional_type(&subject).is_some_and(|kind| kind == "feat") {
        return Ok(CommitPolicy {
            subject,
            bump: Bump::Minor,
        });
    }

    Ok(CommitPolicy {
        subject,
        bump: Bump::Patch,
    })
}

fn conventional_type(subject: &str) -> Option<&str> {
    let (header, _) = subject.split_once(": ")?;
    let header = header.strip_suffix('!').unwrap_or(header);
    let kind = if let Some((kind, scope)) = header.split_once('(') {
        if scope.ends_with(')') {
            kind
        } else {
            return None;
        }
    } else {
        header
    };

    if kind.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-') && !kind.is_empty() {
        Some(kind)
    } else {
        None
    }
}

fn header_is_breaking(subject: &str) -> bool {
    subject
        .split_once(": ")
        .is_some_and(|(header, _)| header.ends_with('!') || header.contains(")!"))
}

fn bumped(version: &Version, bump: Bump) -> Version {
    let mut next = version.clone();
    match bump {
        Bump::Patch => {
            next.patch += 1;
        }
        Bump::Minor => {
            next.minor += 1;
            next.patch = 0;
        }
        Bump::Major => {
            next.major += 1;
            next.minor = 0;
            next.patch = 0;
        }
    }
    next.pre = semver::Prerelease::EMPTY;
    next.build = semver::BuildMetadata::EMPTY;
    next
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

fn cargo_version_from_toml(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let version = document["package"]["version"]
        .as_str()
        .ok_or("Cargo.toml is missing package.version")?;
    Ok(Version::parse(version)?)
}

fn cargo_lock_version(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let packages = document["package"]
        .as_array_of_tables()
        .ok_or("Cargo.lock is missing package table")?;

    for package in packages {
        if package["name"].as_str() == Some("git-ls") {
            let version = package["version"]
                .as_str()
                .ok_or("Cargo.lock git-ls package is missing version")?;
            return Ok(Version::parse(version)?);
        }
    }

    Err("Cargo.lock does not contain git-ls package".into())
}

fn commits_in_range(base: &str, head: &str) -> Result<Vec<String>> {
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

fn parent_count(commit: &str) -> Result<usize> {
    let output = git_output(["rev-list", "--parents", "-n", "1", commit])?;
    Ok(output.split_whitespace().skip(1).count())
}

fn latest_reachable_version_tag(commit: &str) -> Result<Option<String>> {
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

fn existing_tags() -> Result<BTreeMap<String, String>> {
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

fn git_output<I, S>(args: I) -> Result<String>
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

fn run_git<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_command(Command::new("git").args(args))
}

fn run_command(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    Err(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_non_feature_conventional_commits_as_patch() {
        let policy = policy_from_message("chore: update hooks").expect("policy parses");

        assert_eq!(policy.bump, Bump::Patch);
        assert_eq!(policy.subject, "chore: update hooks");
    }

    #[test]
    fn treats_non_conventional_commits_as_patch() {
        let policy = policy_from_message("update hooks").expect("policy parses");

        assert_eq!(policy.bump, Bump::Patch);
    }

    #[test]
    fn treats_feature_commits_as_minor() {
        let policy = policy_from_message("feat(cli): add json output").expect("policy parses");

        assert_eq!(policy.bump, Bump::Minor);
    }

    #[test]
    fn treats_breaking_header_as_major() {
        let policy =
            policy_from_message("feat(cli)!: replace output format").expect("policy parses");

        assert_eq!(policy.bump, Bump::Major);
    }

    #[test]
    fn treats_breaking_footer_as_major() {
        let message = "fix: adjust output\n\nBREAKING CHANGE: output is no longer tabular";
        let policy = policy_from_message(message).expect("policy parses");

        assert_eq!(policy.bump, Bump::Major);
    }

    #[test]
    fn bumps_versions_by_policy() {
        let version = Version::parse("0.2.1").expect("valid version");

        assert_eq!(bumped(&version, Bump::Patch).to_string(), "0.2.2");
        assert_eq!(bumped(&version, Bump::Minor).to_string(), "0.3.0");
        assert_eq!(bumped(&version, Bump::Major).to_string(), "1.0.0");
    }
}

use semver::Version;

use crate::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Bump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitPolicy {
    pub(crate) subject: String,
    pub(crate) bump: Bump,
}

pub(crate) fn policy_from_message(message: &str) -> Result<CommitPolicy> {
    let subject = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or("commit message is empty")?
        .to_string();
    let kind = conventional_type(&subject).ok_or("a Conventional Commit title is required")?;

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

    if kind == "feat" {
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
        if scope
            .strip_suffix(')')
            .is_some_and(|scope| !scope.is_empty() && !scope.contains(['(', ')']))
        {
            kind
        } else {
            return None;
        }
    } else {
        header
    };

    if matches!(
        kind,
        "feat"
            | "fix"
            | "docs"
            | "style"
            | "refactor"
            | "perf"
            | "test"
            | "build"
            | "ci"
            | "chore"
            | "revert"
    ) {
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

pub(crate) fn bumped(version: &Version, bump: Bump) -> Result<Version> {
    let mut next = version.clone();
    match bump {
        Bump::Patch => {
            next.patch = next.patch.checked_add(1).ok_or("patch version overflow")?;
        }
        Bump::Minor => {
            next.minor = next.minor.checked_add(1).ok_or("minor version overflow")?;
            next.patch = 0;
        }
        Bump::Major => {
            next.major = next.major.checked_add(1).ok_or("major version overflow")?;
            next.minor = 0;
            next.patch = 0;
        }
    }
    next.pre = semver::Prerelease::EMPTY;
    next.build = semver::BuildMetadata::EMPTY;
    Ok(next)
}

#[cfg(test)]
#[path = "tests/policy.rs"]
mod tests;

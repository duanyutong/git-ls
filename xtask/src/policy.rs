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
mod tests {
    use super::*;

    #[test]
    fn treats_non_feature_conventional_commits_as_patch() {
        let policy = policy_from_message("chore: update hooks").expect("policy parses");

        assert_eq!(policy.bump, Bump::Patch);
        assert_eq!(policy.subject, "chore: update hooks");
    }

    #[test]
    fn rejects_non_conventional_commits() {
        assert!(policy_from_message("update hooks").is_err());
    }

    #[test]
    fn rejects_malformed_conventional_scopes() {
        for message in [
            "feat(cli: add output",
            "feat(): add output",
            "feat(a(b)): add output",
            "fix: ",
        ] {
            assert!(policy_from_message(message).is_err());
        }
    }

    #[test]
    fn rejects_non_lowercase_conventional_types() {
        assert!(policy_from_message("Feat(cli): add json output").is_err());
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
    fn ignores_comments_and_blank_lines_when_reading_subject() {
        let policy = policy_from_message("\n# generated comment\n\nfix: handle empty input\n")
            .expect("policy parses");

        assert_eq!(policy.subject, "fix: handle empty input");
        assert_eq!(policy.bump, Bump::Patch);
    }

    #[test]
    fn rejects_empty_commit_messages() {
        let error = policy_from_message("\n# comment only\n").unwrap_err();

        assert_eq!(error.to_string(), "commit message is empty");
    }

    #[test]
    fn bumps_versions_by_policy() {
        let version = Version::parse("0.2.1").expect("valid version");

        assert_eq!(bumped(&version, Bump::Patch).unwrap().to_string(), "0.2.2");
        assert_eq!(bumped(&version, Bump::Minor).unwrap().to_string(), "0.3.0");
        assert_eq!(bumped(&version, Bump::Major).unwrap().to_string(), "1.0.0");
    }

    #[test]
    fn rejects_version_overflow() {
        let version = Version::new(u64::MAX, u64::MAX, u64::MAX);
        for bump in [Bump::Patch, Bump::Minor, Bump::Major] {
            assert!(bumped(&version, bump).is_err());
        }
    }
}

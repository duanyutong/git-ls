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

pub(crate) fn bumped(version: &Version, bump: Bump) -> Version {
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
    fn treats_malformed_conventional_scopes_as_patch() {
        let policy = policy_from_message("feat(cli: add json output").expect("policy parses");

        assert_eq!(policy.bump, Bump::Patch);
    }

    #[test]
    fn treats_non_lowercase_conventional_types_as_patch() {
        let policy = policy_from_message("Feat(cli): add json output").expect("policy parses");

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

        assert_eq!(bumped(&version, Bump::Patch).to_string(), "0.2.2");
        assert_eq!(bumped(&version, Bump::Minor).to_string(), "0.3.0");
        assert_eq!(bumped(&version, Bump::Major).to_string(), "1.0.0");
    }
}

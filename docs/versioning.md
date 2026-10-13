# Versioning and release tags

Every commit after the initial commit shall contain the package version required by its Conventional Commit message.
The initial commit establishes the starting version.
`feat` requires a minor increment; `!`, `BREAKING CHANGE:` or `BREAKING-CHANGE:` requires a major increment; all other accepted types require a patch increment.
The manifest and lockfile shall record the same package version.
Release history shall remain linear; merge commits are rejected by version verification.

The commit-message hook generates an omitted increment in `Cargo.toml` and `Cargo.lock`, then stops the commit.
The generated changes require review and staging before the commit is repeated.
Existing unstaged manifest or lockfile changes are preserved and require resolution before automatic generation.
An incorrect staged increment is rejected without replacement.
The pre-push hook and CI independently verify committed version history.

```sh
git add Cargo.toml Cargo.lock
git commit

# Amendment uses the existing commit's parent as the version baseline.
just amend --no-edit

# Verification covers commits after the latest reachable version tag.
just verify-versions
```

Git does not supply an unambiguous amendment flag to commit-message hooks.
The `just amend` recipe invokes `git commit --amend` with the correct version baseline; additional arguments are forwarded to Git.
A change to the commit type may require a corresponding correction to the staged version.

Successful CI for a push to `main` triggers the release workflow.
The workflow verifies the complete untagged range before creating annotated `v<version>` tags, then publishes those tags in one atomic push.
Conflicting tags are rejected; existing tags are never moved.
PR validation and failed CI do not publish tags.
Tag publication does not create a GitHub Release, upload binaries or publish a crate.
The manual equivalent is `just tag-versions <base> <head>`; this command publishes tags to `origin`.

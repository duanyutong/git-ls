#![allow(clippy::expect_used)]

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

pub(super) struct TestRepo {
    directory: TempDir,
}

impl TestRepo {
    pub(super) fn new() -> Self {
        let repo = Self {
            directory: tempfile::tempdir().expect("temporary repository"),
        };
        repo.git(&["-c", "init.templateDir=", "init", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Example Developer"]);
        repo.git(&["config", "user.email", "developer@example.invalid"]);
        repo.git(&["config", "commit.gpgSign", "false"]);
        repo.git(&["config", "tag.gpgSign", "false"]);
        repo
    }

    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(self.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_CONFIG")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                self.path().join("absent-global-config"),
            )
            .env("GIT_AUTHOR_DATE", "2024-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2024-01-01T00:00:00Z");
        command
    }

    pub(super) fn git(&self, args: &[&str]) -> String {
        let output = self.command("git").args(args).assert().success();
        String::from_utf8(output.get_output().stdout.clone())
            .expect("Git output is UTF-8")
            .trim()
            .to_string()
    }

    pub(super) fn commit(&self, subject: &str) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "--allow-empty", "-m", subject]);
        self.git(&["rev-parse", "HEAD"])
    }
}

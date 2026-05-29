use std::collections::{HashMap, HashSet};
use std::process::{Command, Output};

use gix::bstr::ByteSlice as _;

use crate::error::{GitLsError, Result};
use crate::model::{CommitMeta, display_short_oid};

const GIT_SHOW_COMMIT_META_ARG: &str = "--format=%H%x00%ct%x00%s%x1e";

pub(crate) trait GitCommand {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String>;
}

#[derive(Debug, Default)]
pub(crate) struct ProcessGit;

impl GitCommand for ProcessGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let output = execute_git(args)?;

        if !output.status.success() && !allow_failure {
            return Err(GitLsError::git_command(
                args.join(" "),
                command_failure_detail(&output),
            ));
        }

        Ok(normalised_stdout(&output))
    }
}

fn execute_git(args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .output()
        .map_err(GitLsError::GitExec)
}

fn normalised_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string()
}

fn command_failure_detail(output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        detail
    }
}

pub(crate) trait BranchlessQueries {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
}

pub(crate) trait CommitMetadataBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()>;
}

pub(crate) trait RepositoryStateBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>>;
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)>;
    fn main_branch_name(&self) -> Result<String>;
}

pub(crate) trait AncestryBackend {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>>;
    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>>;
}

pub(crate) trait GitBackend:
    BranchlessQueries + CommitMetadataBackend + RepositoryStateBackend + AncestryBackend
{
}

impl<T> GitBackend for T where
    T: BranchlessQueries
        + CommitMetadataBackend
        + RepositoryStateBackend
        + AncestryBackend
        + ?Sized
{
}

impl<T: GitCommand + ?Sized> BranchlessQueries for T {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(self, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(self, revset, hidden)
    }
}

impl<T: GitCommand + ?Sized> CommitMetadataBackend for T {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        shell_cache_commit_metas(self, oids, cache)
    }
}

impl<T: GitCommand + ?Sized> RepositoryStateBackend for T {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        shell_local_branches_by_oid(self)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        shell_current_head_and_branch(self)
    }

    fn main_branch_name(&self) -> Result<String> {
        shell_main_branch_name(self)
    }
}

impl<T: GitCommand + ?Sized> AncestryBackend for T {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        shell_merge_base(self, main_oid, head_oid)
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        shell_ancestry_path(self, base_oid, head_oid)
    }
}

fn shell_query_revset<G: GitCommand + ?Sized>(
    git: &G,
    revset: &str,
    hidden: bool,
) -> Result<Vec<String>> {
    let mut args = vec!["branchless", "query", "-r"];
    if hidden {
        args.push("--hidden");
    }
    args.push(revset);
    Ok(lines(&git.run(&args, false)?))
}

fn shell_query_branch_names<G: GitCommand + ?Sized>(
    git: &G,
    revset: &str,
    hidden: bool,
) -> Result<Vec<String>> {
    let mut args = vec!["branchless", "query", "-b"];
    if hidden {
        args.push("--hidden");
    }
    args.push(revset);
    Ok(lines(&git.run(&args, false)?))
}

fn lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn get_commit_meta<G: CommitMetadataBackend + ?Sized>(
    git: &G,
    oid: &str,
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<CommitMeta> {
    if let Some(meta) = cache.get(oid) {
        return Ok(meta.clone());
    }

    git.cache_commit_metas(&[oid], cache)?;
    cache
        .get(oid)
        .cloned()
        .ok_or_else(|| GitLsError::unexpected_git_show(oid))
}

fn shell_cache_commit_metas<G: GitCommand + ?Sized>(
    git: &G,
    oids: &[&str],
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let missing = missing_commit_aliases(oids, cache);
    if missing.is_empty() {
        return Ok(());
    }

    let mut args = vec!["show", "-s", GIT_SHOW_COMMIT_META_ARG, "--no-walk=unsorted"];
    args.extend(missing.iter().copied());

    let output = git.run(&args, false)?;
    let records = git_show_commit_records(&output);

    if records.len() != missing.len() {
        return Err(GitLsError::unexpected_git_show(missing.join(", ")));
    }

    for (alias, record) in missing.into_iter().zip(records) {
        let meta = parse_shell_commit_meta(alias, record)?;
        insert_commit_meta(cache, alias, meta);
    }

    Ok(())
}

fn missing_commit_aliases<'a>(
    oids: &'a [&str],
    cache: &HashMap<String, CommitMeta>,
) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    oids.iter()
        .copied()
        .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
        .collect()
}

fn git_show_commit_records(output: &str) -> Vec<&str> {
    output
        .split('\x1e')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .map(|record| record.strip_suffix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .collect()
}

fn parse_shell_commit_meta(alias: &str, record: &str) -> Result<CommitMeta> {
    let parts: Vec<&str> = record.splitn(3, '\0').collect();
    if parts.len() != 3 {
        return Err(GitLsError::unexpected_git_show(alias));
    }

    Ok(CommitMeta {
        oid: parts[0].to_string(),
        short_oid: display_short_oid(parts[0]),
        timestamp: parts[1]
            .parse()
            .map_err(|source| GitLsError::invalid_commit_timestamp(alias, source))?,
        subject: parts[2].to_string(),
    })
}

fn insert_commit_meta(cache: &mut HashMap<String, CommitMeta>, alias: &str, meta: CommitMeta) {
    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
}

fn shell_local_branches_by_oid<G: GitCommand + ?Sized>(
    git: &G,
) -> Result<HashMap<String, Vec<String>>> {
    let output = git.run(
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:short)",
            "refs/heads",
        ],
        false,
    )?;
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let Some((oid, branch)) = line.split_once('\0') else {
            continue;
        };
        result
            .entry(oid.to_string())
            .or_default()
            .push(branch.to_string());
    }
    Ok(result)
}

fn shell_current_head_and_branch<G: GitCommand + ?Sized>(
    git: &G,
) -> Result<(Option<String>, Option<String>)> {
    let output = git.run(&["rev-parse", "HEAD", "--abbrev-ref", "HEAD"], true)?;
    let mut values = lines(&output).into_iter();
    let head = values.next();
    let branch = values.next().filter(|branch| branch != "HEAD");
    Ok((head, branch))
}

fn shell_main_branch_name<G: GitCommand + ?Sized>(git: &G) -> Result<String> {
    let output = git.run(&["config", "--get", "branchless.core.mainBranch"], true)?;
    Ok(non_empty(&output).unwrap_or_else(|| "main".to_string()))
}

pub(crate) fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn shell_merge_base<G: GitCommand + ?Sized>(
    git: &G,
    main_oid: &str,
    head_oid: &str,
) -> Result<Option<String>> {
    let output = git.run(&["merge-base", main_oid, head_oid], true)?;
    Ok(non_empty(&output))
}

fn shell_ancestry_path<G: GitCommand + ?Sized>(
    git: &G,
    base_oid: Option<&str>,
    head_oid: &str,
) -> Result<Vec<String>> {
    let output = match base_oid {
        Some(base_oid) => git.run(
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                &format!("{base_oid}..{head_oid}"),
            ],
            false,
        )?,
        None => git.run(&["rev-list", "--reverse", head_oid], false)?,
    };
    Ok(lines(&output))
}

/// Repository backend that uses `gix` for native Git data and shells out only
/// for `git-branchless` revset queries, whose semantics are external to `gix`.
pub(crate) struct GixBackend {
    repo: gix::Repository,
    command: ProcessGit,
}

impl GixBackend {
    pub(crate) fn discover() -> Result<Self> {
        Self::discover_from(".")
    }

    fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut repo = gix::discover_with_environment_overrides(directory)
            .map_err(|source| GitLsError::gix("discover repository", source))?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        Ok(Self {
            repo,
            command: ProcessGit,
        })
    }

    fn object_id(oid: &str) -> Result<gix::ObjectId> {
        oid.parse::<gix::ObjectId>()
            .map_err(|source| GitLsError::invalid_object_id(oid, source))
    }

    fn commit_meta(&self, alias: &str) -> Result<CommitMeta> {
        let oid = Self::object_id(alias)?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|source| GitLsError::gix("find commit", source))?;
        let full_oid = commit.id().detach().to_string();
        let subject = commit
            .message()
            .map_err(|source| GitLsError::gix("read commit message", source))?
            .summary()
            .to_str_lossy()
            .into_owned();
        let timestamp = commit
            .time()
            .map_err(|source| GitLsError::gix("read commit timestamp", source))?
            .seconds;
        let short_oid = display_short_oid(&full_oid);

        Ok(CommitMeta {
            oid: full_oid,
            short_oid,
            subject,
            timestamp,
        })
    }

    fn is_descendant_of(
        &self,
        commit: gix::ObjectId,
        ancestor: gix::ObjectId,
        cache: &mut HashMap<gix::ObjectId, bool>,
    ) -> Result<bool> {
        if commit == ancestor {
            return Ok(true);
        }
        if let Some(result) = cache.get(&commit) {
            return Ok(*result);
        }

        let commit_object = self
            .repo
            .find_commit(commit)
            .map_err(|source| GitLsError::gix("find ancestry commit", source))?;
        for parent in commit_object.parent_ids() {
            let parent = parent.detach();
            if parent == ancestor || self.is_descendant_of(parent, ancestor, cache)? {
                cache.insert(commit, true);
                return Ok(true);
            }
        }

        cache.insert(commit, false);
        Ok(false)
    }
}

impl BranchlessQueries for GixBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(&self.command, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(&self.command, revset, hidden)
    }
}

impl CommitMetadataBackend for GixBackend {
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        for alias in missing_commit_aliases(oids, cache) {
            let meta = self.commit_meta(alias)?;
            insert_commit_meta(cache, alias, meta);
        }
        Ok(())
    }
}

impl RepositoryStateBackend for GixBackend {
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for reference in self
            .repo
            .references()
            .map_err(|source| GitLsError::gix("open references", source))?
            .local_branches()
            .map_err(|source| GitLsError::gix("iterate local branches", source))?
        {
            let reference =
                reference.map_err(|source| GitLsError::gix("read local branch", source))?;
            let Some(id) = reference.try_id() else {
                continue;
            };
            result
                .entry(id.detach().to_string())
                .or_default()
                .push(reference.name().shorten().to_str_lossy().into_owned());
        }
        Ok(result)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        let Ok(head) = self.repo.head_id() else {
            return Ok((None, None));
        };
        let branch = self
            .repo
            .head_name()
            .map_err(|source| GitLsError::gix("read HEAD name", source))?
            .map(|name| name.shorten().to_str_lossy().into_owned());
        Ok((Some(head.detach().to_string()), branch))
    }

    fn main_branch_name(&self) -> Result<String> {
        Ok(self
            .repo
            .config_snapshot()
            .string("branchless.core.mainBranch")
            .map_or_else(
                || "main".to_string(),
                |value| value.to_str_lossy().into_owned(),
            ))
    }
}

impl AncestryBackend for GixBackend {
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        let main_oid = Self::object_id(main_oid)?;
        let head_oid = Self::object_id(head_oid)?;
        match self.repo.merge_base(main_oid, head_oid) {
            Ok(base) => Ok(Some(base.detach().to_string())),
            Err(gix::repository::merge_base::Error::NotFound { .. }) => Ok(None),
            Err(source) => Err(GitLsError::gix("find merge base", source)),
        }
    }

    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>> {
        let head_oid = Self::object_id(head_oid)?;
        let mut walk =
            self.repo
                .rev_walk([head_oid])
                .sorting(gix::revision::walk::Sorting::ByCommitTime(
                    gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
                ));
        let base_oid = base_oid.map(Self::object_id).transpose()?;
        if let Some(base_oid) = base_oid {
            walk = walk.with_hidden([base_oid]);
        }

        let mut descendant_cache = HashMap::new();
        let mut path = Vec::new();
        for info in walk
            .all()
            .map_err(|source| GitLsError::gix("walk revisions", source))?
        {
            let oid = info
                .map_err(|source| GitLsError::gix("read revision walk entry", source))?
                .id;
            let include = match base_oid {
                Some(base) => self.is_descendant_of(oid, base, &mut descendant_cache)?,
                None => true,
            };
            if include {
                path.push(oid.to_string());
            }
        }
        path.reverse();
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command as TestCommand;

    use tempfile::TempDir;

    use super::*;
    use crate::test_support::MockGit;

    #[test]
    fn shell_metadata_hydration_indexes_alias_and_full_oid() {
        let full_oid = "309567f69abcdef0123456789abcdef01234567";
        let git = MockGit::default().with(
            &[
                "show",
                "-s",
                GIT_SHOW_COMMIT_META_ARG,
                "--no-walk=unsorted",
                "branch-head",
            ],
            &format!("{full_oid}\x001700000001\x00subject\x1e"),
        );
        let mut cache = HashMap::new();

        shell_cache_commit_metas(&git, &["branch-head"], &mut cache).unwrap();

        let meta = cache.get("branch-head").unwrap();
        assert_eq!(meta.oid, full_oid);
        assert_eq!(meta.short_oid, "309567f");
        assert_eq!(cache.get(full_oid), Some(meta));
    }

    #[test]
    fn metadata_cache_deduplicates_missing_aliases() {
        let cached_meta = CommitMeta {
            oid: "cached".to_string(),
            short_oid: "cached".to_string(),
            subject: "cached".to_string(),
            timestamp: 1,
        };
        let mut cache = HashMap::new();
        insert_commit_meta(&mut cache, "cached-alias", cached_meta);

        assert_eq!(
            missing_commit_aliases(&["cached", "cached-alias", "new", "new"], &cache),
            vec!["new"]
        );
    }

    #[test]
    fn shell_metadata_hydration_rejects_record_count_mismatch() {
        let full_oid = "309567f69abcdef0123456789abcdef01234567";
        let git = MockGit::default().with(
            &[
                "show",
                "-s",
                GIT_SHOW_COMMIT_META_ARG,
                "--no-walk=unsorted",
                "one",
                "two",
            ],
            &format!("{full_oid}\x001700000001\x00subject\x1e"),
        );
        let mut cache = HashMap::new();

        let error = shell_cache_commit_metas(&git, &["one", "two"], &mut cache).unwrap_err();

        assert!(matches!(
            error,
            GitLsError::UnexpectedGitShow { oid } if oid == "one, two"
        ));
        assert!(cache.is_empty());
    }

    #[test]
    fn shell_metadata_parser_rejects_malformed_records() {
        let error = parse_shell_commit_meta("branch-head", "oid\x001700000001").unwrap_err();

        assert!(matches!(
            error,
            GitLsError::UnexpectedGitShow { oid } if oid == "branch-head"
        ));
    }

    #[test]
    fn shell_metadata_parser_rejects_invalid_timestamps() {
        let error = parse_shell_commit_meta(
            "branch-head",
            "309567f69abcdef0123456789abcdef01234567\x00soon\x00subject",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GitLsError::InvalidCommitTimestamp { oid, .. } if oid == "branch-head"
        ));
    }

    fn run_git(repo: &Path, args: &[&str]) -> String {
        let output = TestCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_string()
    }

    struct TestRepo {
        temp: TempDir,
    }

    impl TestRepo {
        fn init() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let repo = Self { temp };
            repo.git(&["init", "--initial-branch", "main"]);
            repo.git(&["config", "user.name", "git-ls tests"]);
            repo.git(&["config", "user.email", "git-ls@example.invalid"]);
            repo
        }

        fn path(&self) -> &Path {
            self.temp.path()
        }

        fn git(&self, args: &[&str]) -> String {
            run_git(self.path(), args)
        }

        fn commit_file(&self, path: &str, content: &str, message: &str) -> String {
            std::fs::write(self.path().join(path), content).unwrap();
            self.git(&["add", path]);
            self.git(&["commit", "-m", message]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    struct AncestryParityFixture {
        repo: TestRepo,
        base_oid: String,
        head_oid: String,
        side_oid: String,
    }

    fn ancestry_parity_fixture() -> AncestryParityFixture {
        let repo = TestRepo::init();

        repo.commit_file("root.txt", "root\n", "root");
        repo.git(&["checkout", "-b", "side"]);
        let side_oid = repo.commit_file("side.txt", "side\n", "side before base");

        repo.git(&["checkout", "main"]);
        let base_oid = repo.commit_file("base.txt", "base\n", "base");
        repo.git(&["checkout", "-b", "topic"]);
        repo.commit_file("topic.txt", "topic\n", "topic");
        repo.git(&["merge", "--no-ff", "side", "-m", "merge side"]);
        let head_oid = repo.git(&["rev-parse", "HEAD"]);

        AncestryParityFixture {
            repo,
            base_oid,
            head_oid,
            side_oid,
        }
    }

    #[test]
    fn normalises_line_output() {
        assert_eq!(
            lines("  a\n\n b \n\t\nc"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(non_empty("  value \n"), Some("value".to_string()));
        assert_eq!(non_empty(" \n"), None);
    }

    #[test]
    fn gix_backend_matches_git_ancestry_path() {
        let fixture = ancestry_parity_fixture();
        let backend = GixBackend::discover_from(fixture.repo.path()).unwrap();
        let shell_path = lines(&fixture.repo.git(&[
            "rev-list",
            "--reverse",
            "--ancestry-path",
            &format!("{}..{}", fixture.base_oid, fixture.head_oid),
        ]));

        assert_eq!(
            backend
                .merge_base(&fixture.base_oid, &fixture.head_oid)
                .unwrap(),
            Some(fixture.base_oid.clone())
        );
        assert_eq!(
            backend
                .ancestry_path(Some(&fixture.base_oid), &fixture.head_oid)
                .unwrap(),
            shell_path
        );
        assert!(!shell_path.contains(&fixture.side_oid));
    }

    #[test]
    fn gix_backend_reads_repository_snapshot_and_commit_metadata() {
        let repo = TestRepo::init();
        repo.commit_file("root.txt", "root\n", "root");
        repo.git(&["checkout", "-b", "topic"]);
        let head_oid = repo.commit_file("topic.txt", "topic\n", "topic");
        repo.git(&["config", "branchless.core.mainBranch", "trunk"]);

        let backend = GixBackend::discover_from(repo.path()).unwrap();
        let (head, current_branch) = backend.current_head_and_branch().unwrap();
        let branches = backend.local_branches_by_oid().unwrap();
        let mut cache = HashMap::new();
        backend
            .cache_commit_metas(&[&head_oid], &mut cache)
            .unwrap();

        assert_eq!(head, Some(head_oid.clone()));
        assert_eq!(current_branch, Some("topic".to_string()));
        assert!(
            branches
                .get(&head_oid)
                .is_some_and(|names| names.contains(&"topic".to_string()))
        );
        assert_eq!(backend.main_branch_name().unwrap(), "trunk");
        let meta = cache.get(&head_oid).unwrap();
        assert_eq!(meta.short_oid, display_short_oid(&head_oid));
        assert_eq!(meta.subject, "topic");
    }

    #[test]
    fn gix_metadata_hydration_rejects_malformed_oid() {
        let error = GixBackend::object_id("not-an-oid").unwrap_err();

        assert!(matches!(
            error,
            GitLsError::InvalidObjectId { oid, .. } if oid == "not-an-oid"
        ));
    }

    #[test]
    fn gix_metadata_hydration_reports_missing_commits() {
        let repo = TestRepo::init();
        let backend = GixBackend::discover_from(repo.path()).unwrap();
        let mut cache = HashMap::new();

        let error = backend
            .cache_commit_metas(&["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"], &mut cache)
            .unwrap_err();

        assert!(matches!(
            error,
            GitLsError::Gix {
                context: "find commit",
                ..
            }
        ));
        assert!(cache.is_empty());
    }
}

use anstyle::{Ansi256Color, Style};
use clap::{Parser, ValueEnum};
use gix::bstr::ByteSlice as _;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::num::ParseIntError;
use std::process::Command;
use thiserror::Error;

const PALETTE: [u8; 10] = [39, 208, 141, 82, 203, 220, 45, 177, 114, 214];
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git ",
    env!("VERGEN_GIT_SHA"),
    ", dirty=",
    env!("VERGEN_GIT_DIRTY"),
    ", target=",
    env!("VERGEN_CARGO_TARGET_TRIPLE"),
    ", rustc=",
    env!("VERGEN_RUSTC_SEMVER"),
    ", built=",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")"
);

#[derive(Debug, Error)]
pub enum GitLsError {
    #[error(transparent)]
    Cli(#[from] clap::Error),

    #[error("failed to execute git: {0}")]
    GitExec(#[source] io::Error),

    #[error("git {args} failed: {detail}")]
    GitCommand { args: String, detail: String },

    #[error("gix {context} failed: {detail}")]
    Gix {
        context: &'static str,
        detail: String,
    },

    #[error("invalid git object id {oid}: {detail}")]
    InvalidObjectId { oid: String, detail: String },

    #[error("unexpected git show output for {oid}")]
    UnexpectedGitShow { oid: String },

    #[error("invalid commit timestamp for {oid}: {source}")]
    InvalidCommitTimestamp {
        oid: String,
        #[source]
        source: ParseIntError,
    },

    #[error("expected main() to resolve to one commit, got {count}")]
    AmbiguousMainRevset { count: usize },

    #[error("failed to write output: {0}")]
    Write(#[from] io::Error),

    #[cfg(test)]
    #[error("{0}")]
    TestFixture(String),
}

pub type Result<T> = std::result::Result<T, GitLsError>;

trait GitCommand {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String>;
}

#[derive(Debug, Default)]
struct ProcessGit;

impl GitCommand for ProcessGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(GitLsError::GitExec)?;

        if !output.status.success() && !allow_failure {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let fallback = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if detail.is_empty() { fallback } else { detail };
            return Err(GitLsError::GitCommand {
                args: args.join(" "),
                detail,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string())
    }
}

trait GitBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>>;
    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()>;
    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>>;
    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)>;
    fn main_branch_name(&self) -> Result<String>;
    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>>;
    fn ancestry_path(&self, base_oid: Option<&str>, head_oid: &str) -> Result<Vec<String>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommitMeta {
    oid: String,
    short_oid: String,
    subject: String,
    timestamp: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BranchPoint {
    oid: String,
    names: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Lane {
    head_oid: String,
    base_oid: Option<String>,
    branch_points: Vec<BranchPoint>,
    head_timestamp: i64,
    contains_current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RepositorySnapshot {
    branches_by_oid: HashMap<String, Vec<String>>,
    current_branch: Option<String>,
    head: Option<String>,
    main_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BuiltLanes {
    Empty,
    Populated {
        lanes: Vec<Lane>,
        main_oid: String,
        repository: RepositorySnapshot,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ColourMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Order {
    Newest,
    Oldest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Backend {
    Gix,
    Shell,
}

#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "git ls",
    about = "Render git-branchless draft branches as coloured stack lanes.",
    version = VERSION
)]
struct Args {
    #[arg(default_value = "draft()", value_name = "REVSET")]
    revset: String,

    #[arg(long)]
    hidden: bool,

    #[arg(long, value_enum, default_value = "gix", value_name = "VALUE")]
    backend: Backend,

    #[arg(long, value_enum, default_value = "newest", value_name = "VALUE")]
    order: Order,

    #[arg(
        long = "color",
        alias = "colour",
        value_enum,
        default_value = "auto",
        value_name = "VALUE"
    )]
    colour_mode: ColourMode,
}

#[derive(Debug)]
struct Colours {
    enabled: bool,
}

impl Colours {
    fn new(mode: ColourMode) -> Self {
        let enabled = match mode {
            ColourMode::Auto => std::io::stdout().is_terminal(),
            ColourMode::Always => true,
            ColourMode::Never => false,
        };
        Self { enabled }
    }

    fn paint(&self, text: &str, style: Style) -> String {
        if !self.enabled || text.is_empty() {
            text.to_string()
        } else {
            format!("{style}{text}{style:#}")
        }
    }

    fn stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(PALETTE[index % PALETTE.len()]).on_default(),
        )
    }

    fn dim(&self, text: &str) -> String {
        self.paint(text, Style::new().dimmed())
    }
}

impl<T: GitCommand + ?Sized> GitBackend for T {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(self, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(self, revset, hidden)
    }

    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        shell_cache_commit_metas(self, oids, cache)
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        shell_local_branches_by_oid(self)
    }

    fn current_head_and_branch(&self) -> Result<(Option<String>, Option<String>)> {
        shell_current_head_and_branch(self)
    }

    fn main_branch_name(&self) -> Result<String> {
        shell_main_branch_name(self)
    }

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

fn get_commit_meta<G: GitBackend + ?Sized>(
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
        .ok_or_else(|| GitLsError::UnexpectedGitShow {
            oid: oid.to_string(),
        })
}

fn shell_cache_commit_metas<G: GitCommand + ?Sized>(
    git: &G,
    oids: &[&str],
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let mut seen = HashSet::new();
    let missing: Vec<&str> = oids
        .iter()
        .copied()
        .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "show",
        "-s",
        "--format=%H%x00%h%x00%ct%x00%s%x1e",
        "--no-walk=unsorted",
    ];
    args.extend(missing.iter().copied());

    let output = git.run(&args, false)?;
    let records: Vec<&str> = output
        .split('\x1e')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .map(|record| record.strip_suffix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .collect();

    if records.len() != missing.len() {
        return Err(GitLsError::UnexpectedGitShow {
            oid: missing.join(", "),
        });
    }

    for (alias, record) in missing.into_iter().zip(records) {
        shell_cache_commit_meta(alias, record, cache)?;
    }

    Ok(())
}

fn shell_cache_commit_meta(
    alias: &str,
    record: &str,
    cache: &mut HashMap<String, CommitMeta>,
) -> Result<()> {
    let parts: Vec<&str> = record.splitn(4, '\0').collect();
    if parts.len() != 4 {
        return Err(GitLsError::UnexpectedGitShow {
            oid: alias.to_string(),
        });
    }

    let meta = CommitMeta {
        oid: parts[0].to_string(),
        short_oid: parts[1].to_string(),
        timestamp: parts[2]
            .parse()
            .map_err(|source| GitLsError::InvalidCommitTimestamp {
                oid: alias.to_string(),
                source,
            })?,
        subject: parts[3].to_string(),
    };

    if alias != meta.oid {
        cache.insert(alias.to_string(), meta.clone());
    }
    cache.insert(meta.oid.clone(), meta);
    Ok(())
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

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn branch_revset(user_revset: &str) -> String {
    format!("(({user_revset}) & branches()) - public()")
}

fn branch_points_by_oid(
    branch_names: &[String],
    branch_oid_map: &HashMap<String, Vec<String>>,
) -> HashMap<String, BranchPoint> {
    let selected: HashSet<&str> = branch_names.iter().map(String::as_str).collect();
    let mut result = HashMap::new();

    for (oid, names) in branch_oid_map {
        let mut point_names: Vec<String> = names
            .iter()
            .filter(|name| selected.contains(name.as_str()))
            .cloned()
            .collect();
        if point_names.is_empty() {
            continue;
        }
        point_names.sort();

        result.insert(
            oid.clone(),
            BranchPoint {
                oid: oid.clone(),
                names: point_names,
            },
        );
    }

    result
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

#[derive(Debug)]
struct GixBackend {
    repo: gix::Repository,
    command: ProcessGit,
}

impl GixBackend {
    fn discover() -> Result<Self> {
        Self::discover_from(".")
    }

    fn discover_from(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut repo = gix::discover_with_environment_overrides(directory)
            .map_err(|source| gix_error("discover repository", source))?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        Ok(Self {
            repo,
            command: ProcessGit,
        })
    }

    fn object_id(oid: &str) -> Result<gix::ObjectId> {
        oid.parse::<gix::ObjectId>()
            .map_err(|source| GitLsError::InvalidObjectId {
                oid: oid.to_string(),
                detail: source.to_string(),
            })
    }

    fn commit_meta(&self, alias: &str) -> Result<CommitMeta> {
        let oid = Self::object_id(alias)?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|source| gix_error("find commit", source))?;
        let full_oid = commit.id().detach();
        let subject = commit
            .message()
            .map_err(|source| gix_error("read commit message", source))?
            .summary()
            .to_str_lossy()
            .into_owned();
        let timestamp = commit
            .time()
            .map_err(|source| gix_error("read commit timestamp", source))?
            .seconds;
        let short_oid = commit
            .short_id()
            .map_err(|source| gix_error("shorten commit id", source))?
            .to_string();

        Ok(CommitMeta {
            oid: full_oid.to_string(),
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
            .map_err(|source| gix_error("find ancestry commit", source))?;
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

impl GitBackend for GixBackend {
    fn query_revset(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_revset(&self.command, revset, hidden)
    }

    fn query_branch_names(&self, revset: &str, hidden: bool) -> Result<Vec<String>> {
        shell_query_branch_names(&self.command, revset, hidden)
    }

    fn cache_commit_metas(
        &self,
        oids: &[&str],
        cache: &mut HashMap<String, CommitMeta>,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let missing: Vec<&str> = oids
            .iter()
            .copied()
            .filter(|oid| !cache.contains_key(*oid) && seen.insert(*oid))
            .collect();
        for alias in missing {
            let meta = self.commit_meta(alias)?;
            if alias != meta.oid {
                cache.insert(alias.to_string(), meta.clone());
            }
            cache.insert(meta.oid.clone(), meta);
        }
        Ok(())
    }

    fn local_branches_by_oid(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for reference in self
            .repo
            .references()
            .map_err(|source| gix_error("open references", source))?
            .local_branches()
            .map_err(|source| gix_error("iterate local branches", source))?
        {
            let reference = reference.map_err(|source| gix_error("read local branch", source))?;
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
            .map_err(|source| gix_error("read HEAD name", source))?
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

    fn merge_base(&self, main_oid: &str, head_oid: &str) -> Result<Option<String>> {
        let main_oid = Self::object_id(main_oid)?;
        let head_oid = Self::object_id(head_oid)?;
        match self.repo.merge_base(main_oid, head_oid) {
            Ok(base) => Ok(Some(base.detach().to_string())),
            Err(gix::repository::merge_base::Error::NotFound { .. }) => Ok(None),
            Err(source) => Err(gix_error("find merge base", source)),
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
            .map_err(|source| gix_error("walk revisions", source))?
        {
            let oid = info
                .map_err(|source| gix_error("read revision walk entry", source))?
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

fn gix_error(context: &'static str, source: impl std::fmt::Display) -> GitLsError {
    GitLsError::Gix {
        context,
        detail: source.to_string(),
    }
}

fn build_lane<G: GitBackend + ?Sized>(
    git: &G,
    head_oid: &str,
    main_oid: &str,
    points_by_oid: &HashMap<String, BranchPoint>,
    current_branch: Option<&str>,
    head: Option<&str>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Option<Lane>> {
    let base_oid = git.merge_base(main_oid, head_oid)?;
    let path = git.ancestry_path(base_oid.as_deref(), head_oid)?;
    let mut branch_points: Vec<BranchPoint> = path
        .iter()
        .filter_map(|oid| points_by_oid.get(oid).cloned())
        .collect();

    if branch_points.is_empty()
        && let Some(point) = points_by_oid.get(head_oid)
    {
        branch_points.push(point.clone());
    }
    if branch_points.is_empty() {
        return Ok(None);
    }

    branch_points.reverse();
    let head_meta = get_commit_meta(git, head_oid, meta_cache)?;
    let contains_current = branch_points.iter().any(|point| {
        current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
            || head.is_some_and(|head| point.oid == head)
    });

    Ok(Some(Lane {
        head_oid: head_meta.oid,
        base_oid,
        branch_points,
        head_timestamp: head_meta.timestamp,
        contains_current,
    }))
}

fn build_lanes<G: GitBackend + ?Sized>(
    git: &G,
    args: &Args,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<BuiltLanes> {
    let revset = branch_revset(&args.revset);
    let branch_names = git.query_branch_names(&revset, args.hidden)?;
    if branch_names.is_empty() {
        return Ok(BuiltLanes::Empty);
    }

    let main_oids = git.query_revset("main()", args.hidden)?;
    if main_oids.len() != 1 {
        return Err(GitLsError::AmbiguousMainRevset {
            count: main_oids.len(),
        });
    }
    let main_oid = main_oids[0].clone();

    let branch_oid_map = git.local_branches_by_oid()?;
    let points_by_oid = branch_points_by_oid(&branch_names, &branch_oid_map);
    let heads_revset = format!("heads({revset})");
    let head_oids = git.query_revset(&heads_revset, args.hidden)?;
    let (head, current_branch) = git.current_head_and_branch()?;
    let main_name = git.main_branch_name()?;

    let head_refs: Vec<&str> = head_oids.iter().map(String::as_str).collect();
    git.cache_commit_metas(&head_refs, meta_cache)?;

    let mut lanes = Vec::new();
    for head_oid in head_oids {
        if let Some(lane) = build_lane(
            git,
            &head_oid,
            &main_oid,
            &points_by_oid,
            current_branch.as_deref(),
            head.as_deref(),
            meta_cache,
        )? {
            lanes.push(lane);
        }
    }

    Ok(BuiltLanes::Populated {
        lanes,
        main_oid,
        repository: RepositorySnapshot {
            branches_by_oid: branch_oid_map,
            current_branch,
            head,
            main_name,
        },
    })
}

fn ordered_lanes(mut lanes: Vec<Lane>, order: Order) -> Vec<Lane> {
    lanes.sort_by(|lhs, rhs| {
        (!lhs.contains_current)
            .cmp(&!rhs.contains_current)
            .then_with(|| match order {
                Order::Newest => rhs.head_timestamp.cmp(&lhs.head_timestamp),
                Order::Oldest => lhs.head_timestamp.cmp(&rhs.head_timestamp),
            })
            .then_with(|| lhs.head_oid.cmp(&rhs.head_oid))
    });
    lanes
}

fn grouped_by_base(lanes: Vec<Lane>) -> Vec<(Option<String>, Vec<Lane>)> {
    let mut groups: HashMap<Option<String>, Vec<Lane>> = HashMap::new();
    for lane in lanes {
        groups.entry(lane.base_oid.clone()).or_default().push(lane);
    }

    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|lhs, rhs| {
        let lhs_contains_current = lhs.1.iter().any(|lane| lane.contains_current);
        let rhs_contains_current = rhs.1.iter().any(|lane| lane.contains_current);
        (!lhs_contains_current)
            .cmp(&!rhs_contains_current)
            .then_with(|| rhs.1.len().cmp(&lhs.1.len()))
            .then_with(|| {
                lhs.0
                    .as_deref()
                    .unwrap_or("")
                    .cmp(rhs.0.as_deref().unwrap_or(""))
            })
    });
    groups
}

fn marker_for(
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
) -> &'static str {
    if head.is_some_and(|head| point.oid == head)
        || current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
    {
        "◉"
    } else {
        "◯"
    }
}

fn display_names(point: &BranchPoint, current_branch: Option<&str>) -> String {
    point
        .names
        .iter()
        .map(|name| {
            if current_branch.is_some_and(|branch| branch == name) {
                format!("ᐅ {name}")
            } else {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn row_prefix(
    lane_index: usize,
    lane_count: usize,
    colour_offset: usize,
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
    colours: &Colours,
) -> String {
    let mut slots = Vec::new();
    for index in 0..lane_count {
        let colour_index = colour_offset + index;
        match index.cmp(&lane_index) {
            Ordering::Less => slots.push(colours.stack(colour_index, "│")),
            Ordering::Equal => {
                slots.push(colours.stack(colour_index, marker_for(point, current_branch, head)));
            }
            Ordering::Greater => slots.push(" ".to_string()),
        }
    }
    format!(" {}", slots.join(" "))
}

fn base_label<G: GitBackend + ?Sized>(
    git: &G,
    base_oid: Option<&str>,
    main_oid: &str,
    main_name: &str,
    branches_by_oid: &HashMap<String, Vec<String>>,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<String> {
    let Some(base_oid) = base_oid else {
        return Ok("<no merge base>".to_string());
    };
    if base_oid == main_oid {
        return Ok(main_name.to_string());
    }

    let mut branch_names: Vec<String> = branches_by_oid
        .get(base_oid)
        .into_iter()
        .flatten()
        .filter(|name| name.as_str() != main_name)
        .cloned()
        .collect();
    branch_names.sort();
    if !branch_names.is_empty() {
        return Ok(branch_names.join(", "));
    }

    let meta = get_commit_meta(git, base_oid, meta_cache)?;
    Ok(format!("{} {}", meta.short_oid, meta.subject))
}

fn trunk_prefix(lane_count: usize, colour_offset: usize, colours: &Colours) -> String {
    if lane_count <= 1 {
        return format!(" {}", colours.dim("◯"));
    }

    let mut parts = vec![colours.dim("◯")];
    for index in 1..lane_count {
        let connector = if index == lane_count - 1 {
            "─┘"
        } else {
            "─┴"
        };
        parts.push(colours.stack(colour_offset + index, connector));
    }
    format!(" {}", parts.join(""))
}

struct RenderContext<'a, G: GitBackend + ?Sized> {
    git: &'a G,
    main_oid: &'a str,
    main_name: &'a str,
    branches_by_oid: &'a HashMap<String, Vec<String>>,
    current_branch: Option<&'a str>,
    head: Option<&'a str>,
    colours: &'a Colours,
    meta_cache: &'a mut HashMap<String, CommitMeta>,
}

fn render_group<G: GitBackend + ?Sized>(
    lanes: &[Lane],
    base_oid: Option<&str>,
    colour_offset: usize,
    ctx: &mut RenderContext<'_, G>,
) -> Result<Vec<String>> {
    let lane_count = lanes.len();
    let mut output = Vec::new();

    for (lane_index, lane) in lanes.iter().enumerate() {
        for point in &lane.branch_points {
            let prefix = row_prefix(
                lane_index,
                lane_count,
                colour_offset,
                point,
                ctx.current_branch,
                ctx.head,
                ctx.colours,
            );
            let label = ctx.colours.stack(
                colour_offset + lane_index,
                &display_names(point, ctx.current_branch),
            );
            output.push(format!("{prefix}  {label}"));
        }
    }

    let label = base_label(
        ctx.git,
        base_oid,
        ctx.main_oid,
        ctx.main_name,
        ctx.branches_by_oid,
        ctx.meta_cache,
    )?;
    output.push(format!(
        "{}  {}",
        trunk_prefix(lane_count, colour_offset, ctx.colours),
        ctx.colours.dim(&label)
    ));
    Ok(output)
}

fn parse_args_from<I, S>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let cli_args =
        std::iter::once(OsString::from("git ls")).chain(args.into_iter().map(Into::into));
    Args::try_parse_from(cli_args).map_err(Into::into)
}

fn execute<W, G>(args: &Args, git: &G, stdout: &mut W) -> Result<()>
where
    W: Write,
    G: GitBackend + ?Sized,
{
    let colours = Colours::new(args.colour_mode);

    let mut meta_cache = HashMap::new();
    let BuiltLanes::Populated {
        lanes,
        main_oid,
        repository,
    } = build_lanes(git, args, &mut meta_cache)?
    else {
        writeln!(stdout, "No draft branches matched.")?;
        return Ok(());
    };

    let lanes = ordered_lanes(lanes, args.order);

    let mut ctx = RenderContext {
        git,
        main_oid: &main_oid,
        main_name: &repository.main_name,
        branches_by_oid: &repository.branches_by_oid,
        current_branch: repository.current_branch.as_deref(),
        head: repository.head.as_deref(),
        colours: &colours,
        meta_cache: &mut meta_cache,
    };

    let mut first_group = true;
    let mut colour_offset = 0;
    for (base_oid, base_lanes) in grouped_by_base(lanes) {
        if !first_group {
            writeln!(stdout)?;
        }
        first_group = false;
        for line in render_group(&base_lanes, base_oid.as_deref(), colour_offset, &mut ctx)? {
            writeln!(stdout, "{line}")?;
        }
        colour_offset += base_lanes.len();
    }

    Ok(())
}

#[cfg(test)]
fn run<I, S, W, G>(args: I, git: &G, stdout: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    G: GitBackend + ?Sized,
{
    let args = parse_args_from(args)?;
    execute(&args, git, stdout)
}

pub fn run_from_env() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let args = parse_args_from(env::args().skip(1))?;
    match args.backend {
        Backend::Gix => {
            let git = GixBackend::discover()?;
            execute(&args, &git, &mut stdout)
        }
        Backend::Shell => {
            let git = ProcessGit;
            execute(&args, &git, &mut stdout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use std::cell::RefCell;
    use std::path::Path;
    use std::process::Command as TestCommand;
    use tempfile::TempDir;

    #[derive(Default)]
    struct MockGit {
        responses: HashMap<Vec<String>, String>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl MockGit {
        fn with(mut self, args: &[&str], output: &str) -> Self {
            self.responses.insert(
                args.iter().map(|arg| (*arg).to_string()).collect(),
                output.to_string(),
            );
            self
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }
    }

    impl GitCommand for MockGit {
        fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
            let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            self.calls.borrow_mut().push(key.clone());
            if let Some(output) = self.responses.get(&key) {
                return Ok(output.clone());
            }
            if allow_failure {
                return Ok(String::new());
            }
            Err(GitLsError::TestFixture(format!(
                "missing mock git response: {}",
                args.join(" ")
            )))
        }
    }

    fn clap_error_kind(error: GitLsError) -> ErrorKind {
        match error {
            GitLsError::Cli(error) => error.kind(),
            error => panic!("expected clap error, got {error:?}"),
        }
    }

    fn point(oid: &str, names: &[&str]) -> BranchPoint {
        BranchPoint {
            oid: oid.to_string(),
            names: names.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    fn lane(oid: &str, base: Option<&str>, timestamp: i64, contains_current: bool) -> Lane {
        Lane {
            head_oid: oid.to_string(),
            base_oid: base.map(ToOwned::to_owned),
            branch_points: vec![point(oid, &[oid])],
            head_timestamp: timestamp,
            contains_current,
        }
    }

    fn git(repo: &Path, args: &[&str]) -> String {
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

    fn commit_file(repo: &Path, path: &str, content: &str, message: &str) -> String {
        std::fs::write(repo.join(path), content).unwrap();
        git(repo, &["add", path]);
        git(repo, &["commit", "-m", message]);
        git(repo, &["rev-parse", "HEAD"])
    }

    fn parity_repo() -> (TempDir, String, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        git(repo, &["init", "--initial-branch", "main"]);
        git(repo, &["config", "user.name", "git-ls tests"]);
        git(repo, &["config", "user.email", "git-ls@example.invalid"]);

        commit_file(repo, "root.txt", "root\n", "root");
        git(repo, &["checkout", "-b", "side"]);
        let side_oid = commit_file(repo, "side.txt", "side\n", "side before base");

        git(repo, &["checkout", "main"]);
        let base_oid = commit_file(repo, "base.txt", "base\n", "base");
        git(repo, &["checkout", "-b", "topic"]);
        commit_file(repo, "topic.txt", "topic\n", "topic");
        git(repo, &["merge", "--no-ff", "side", "-m", "merge side"]);
        let head_oid = git(repo, &["rev-parse", "HEAD"]);

        (temp, base_oid, head_oid, side_oid)
    }

    #[test]
    fn parses_default_arguments() {
        assert_eq!(
            parse_args_from(Vec::<String>::new()).unwrap(),
            Args {
                revset: "draft()".to_string(),
                hidden: false,
                backend: Backend::Gix,
                order: Order::Newest,
                colour_mode: ColourMode::Auto,
            }
        );
    }

    #[test]
    fn parses_flags_and_revset() {
        assert_eq!(
            parse_args_from([
                "--hidden",
                "--backend=shell",
                "--order=oldest",
                "--colour",
                "never",
                "draft() & branches(feature/)",
            ])
            .unwrap(),
            Args {
                revset: "draft() & branches(feature/)".to_string(),
                hidden: true,
                backend: Backend::Shell,
                order: Order::Oldest,
                colour_mode: ColourMode::Never,
            }
        );
    }

    #[test]
    fn parses_dash_prefixed_revset_after_separator() {
        assert_eq!(
            parse_args_from(["--", "-synthetic-revset"]).unwrap(),
            Args {
                revset: "-synthetic-revset".to_string(),
                hidden: false,
                backend: Backend::Gix,
                order: Order::Newest,
                colour_mode: ColourMode::Auto,
            }
        );
    }

    #[test]
    fn parses_help_without_requiring_git() {
        assert_eq!(
            clap_error_kind(parse_args_from(["--help"]).unwrap_err()),
            ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn rejects_invalid_arguments() {
        assert_eq!(
            clap_error_kind(parse_args_from(["--order"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--order=later"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--colour=maybe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--unknown"]).unwrap_err()),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["one", "two"]).unwrap_err()),
            ErrorKind::UnknownArgument
        );
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
    fn creates_branch_revset() {
        assert_eq!(
            branch_revset("draft()"),
            "((draft()) & branches()) - public()"
        );
    }

    #[test]
    fn maps_selected_branch_points_by_oid() {
        let mut branches = HashMap::new();
        branches.insert(
            "a".to_string(),
            vec!["zeta".to_string(), "alpha".to_string(), "main".to_string()],
        );
        branches.insert("b".to_string(), vec!["ignored".to_string()]);

        let selected = vec!["alpha".to_string(), "zeta".to_string()];
        let points = branch_points_by_oid(&selected, &branches);

        assert_eq!(points.len(), 1);
        assert_eq!(
            points.get("a").unwrap(),
            &BranchPoint {
                oid: "a".to_string(),
                names: vec!["alpha".to_string(), "zeta".to_string()],
            }
        );
    }

    #[test]
    fn gix_backend_matches_git_ancestry_path() {
        let (temp, base_oid, head_oid, side_oid) = parity_repo();
        let repo = temp.path();
        let backend = GixBackend::discover_from(repo).unwrap();
        let shell_path = lines(&git(
            repo,
            &[
                "rev-list",
                "--reverse",
                "--ancestry-path",
                &format!("{base_oid}..{head_oid}"),
            ],
        ));

        assert_eq!(
            backend.merge_base(&base_oid, &head_oid).unwrap(),
            Some(base_oid.clone())
        );
        assert_eq!(
            backend.ancestry_path(Some(&base_oid), &head_oid).unwrap(),
            shell_path
        );
        assert!(!shell_path.contains(&side_oid));
    }

    #[test]
    fn gix_backend_reads_repository_snapshot_and_commit_metadata() {
        let (temp, _base_oid, head_oid, _side_oid) = parity_repo();
        let repo = temp.path();
        git(repo, &["config", "branchless.core.mainBranch", "trunk"]);

        let backend = GixBackend::discover_from(repo).unwrap();
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
        assert_eq!(cache.get(&head_oid).unwrap().subject, "merge side");
    }

    #[test]
    fn orders_lanes_by_current_status_time_and_oid() {
        let lanes = vec![
            lane("older", Some("main"), 10, false),
            lane("current", Some("main"), 1, true),
            lane("newer-b", Some("main"), 20, false),
            lane("newer-a", Some("main"), 20, false),
        ];

        let newest: Vec<String> = ordered_lanes(lanes.clone(), Order::Newest)
            .into_iter()
            .map(|lane| lane.head_oid)
            .collect();
        assert_eq!(newest, vec!["current", "newer-a", "newer-b", "older"]);

        let oldest: Vec<String> = ordered_lanes(lanes, Order::Oldest)
            .into_iter()
            .map(|lane| lane.head_oid)
            .collect();
        assert_eq!(oldest, vec!["current", "older", "newer-a", "newer-b"]);
    }

    #[test]
    fn groups_lanes_by_base_with_current_group_first() {
        let lanes = vec![
            lane("a", Some("base-a"), 1, false),
            lane("b", Some("base-b"), 2, true),
            lane("c", Some("base-a"), 3, false),
        ];

        let groups = grouped_by_base(lanes);

        assert_eq!(groups[0].0, Some("base-b".to_string()));
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[1].0, Some("base-a".to_string()));
        assert_eq!(groups[1].1.len(), 2);
    }

    #[test]
    fn renders_markers_names_and_trunk() {
        let git = MockGit::default();
        let colours = Colours { enabled: false };
        let branches_by_oid = HashMap::new();
        let mut meta_cache = HashMap::new();
        let lanes = vec![
            Lane {
                head_oid: "a".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("a", &["feature/one"])],
                head_timestamp: 1,
                contains_current: false,
            },
            Lane {
                head_oid: "b".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("b", &["feature/two"])],
                head_timestamp: 2,
                contains_current: true,
            },
        ];
        let mut ctx = RenderContext {
            git: &git,
            main_oid: "main",
            main_name: "main",
            branches_by_oid: &branches_by_oid,
            current_branch: Some("feature/two"),
            head: Some("b"),
            colours: &colours,
            meta_cache: &mut meta_cache,
        };

        let output = render_group(&lanes, Some("main"), 0, &mut ctx).unwrap();

        assert_eq!(output.len(), 3);
        assert!(output[0].contains("feature/one"));
        assert!(output[1].contains("ᐅ feature/two"));
        assert_eq!(output[2], " ◯─┘  main");
    }

    #[test]
    fn renders_base_labels_without_unnecessary_git_calls() {
        let git = MockGit::default();
        let mut branches_by_oid = HashMap::new();
        branches_by_oid.insert(
            "base".to_string(),
            vec!["main".to_string(), "topic".to_string()],
        );
        let mut cache = HashMap::new();

        assert_eq!(
            base_label(&git, None, "main", "main", &branches_by_oid, &mut cache).unwrap(),
            "<no merge base>"
        );
        assert_eq!(
            base_label(
                &git,
                Some("main"),
                "main",
                "main",
                &branches_by_oid,
                &mut cache
            )
            .unwrap(),
            "main"
        );
        assert_eq!(
            base_label(
                &git,
                Some("base"),
                "main",
                "main",
                &branches_by_oid,
                &mut cache
            )
            .unwrap(),
            "topic"
        );
        assert!(git.calls().is_empty());
    }

    #[test]
    fn colours_text_when_enabled() {
        let colours = Colours { enabled: true };

        assert_eq!(colours.stack(0, "x"), "\x1b[38;5;39mx\x1b[0m");
        assert_eq!(colours.dim("x"), "\x1b[2mx\x1b[0m");
    }

    #[test]
    fn builds_lanes_with_mocked_git_boundary() {
        let revset = "((draft()) & branches()) - public()";
        let git = MockGit::default()
            .with(&["branchless", "query", "-r", "main()"], "main-oid")
            .with(
                &["branchless", "query", "-b", revset],
                "feature/one\nfeature/two\nchore/other",
            )
            .with(
                &[
                    "for-each-ref",
                    "--format=%(objectname)%00%(refname:short)",
                    "refs/heads",
                ],
                "a\0feature/one\nb\0feature/two\nc\0chore/other\nmain-oid\0main",
            )
            .with(
                &["branchless", "query", "-r", &format!("heads({revset})")],
                "b\nc",
            )
            .with(
                &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
                "b\nfeature/two",
            )
            .with(&["config", "--get", "branchless.core.mainBranch"], "")
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%h%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "b",
                    "c",
                ],
                "b\x00b\x001700000002\x00second\x1e\nc\x00c\x001700000001\x00third\x1e",
            )
            .with(&["merge-base", "main-oid", "b"], "main-oid")
            .with(
                &["rev-list", "--reverse", "--ancestry-path", "main-oid..b"],
                "a\nb",
            )
            .with(&["merge-base", "main-oid", "c"], "main-oid")
            .with(
                &["rev-list", "--reverse", "--ancestry-path", "main-oid..c"],
                "c",
            );
        let args = Args {
            revset: "draft()".to_string(),
            hidden: false,
            backend: Backend::Gix,
            order: Order::Newest,
            colour_mode: ColourMode::Never,
        };
        let mut cache = HashMap::new();

        let BuiltLanes::Populated {
            lanes,
            main_oid,
            repository,
        } = build_lanes(&git, &args, &mut cache).unwrap()
        else {
            panic!("expected populated lanes");
        };

        assert_eq!(main_oid, "main-oid");
        assert_eq!(repository.main_name, "main");
        assert_eq!(lanes.len(), 2);
        assert_eq!(lanes[0].head_oid, "b");
        assert!(lanes[0].contains_current);
        assert_eq!(
            lanes[0].branch_points,
            vec![point("b", &["feature/two"]), point("a", &["feature/one"])]
        );
        assert_eq!(lanes[1].head_oid, "c");
        assert!(!lanes[1].contains_current);

        let calls = git.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.first().is_some_and(|arg| arg == "for-each-ref"))
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call
                    .get(2)
                    .is_some_and(|arg| arg == "--format=%H%x00%h%x00%ct%x00%s%x1e"))
                .count(),
            1
        );
    }

    #[test]
    fn run_reports_empty_selection() {
        let revset = "((draft()) & branches()) - public()";
        let git = MockGit::default()
            .with(&["branchless", "query", "-r", "main()"], "main-oid")
            .with(&["branchless", "query", "-b", revset], "");
        let mut output = Vec::new();

        run(["--color", "never"], &git, &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "No draft branches matched.\n"
        );
        assert!(
            git.calls()
                .iter()
                .all(|call| call.first().is_none_or(|arg| arg != "for-each-ref"))
        );
        assert!(
            git.calls()
                .iter()
                .all(|call| call.get(3).is_none_or(|arg| arg != "main()"))
        );
        assert_eq!(
            git.calls(),
            vec![vec![
                "branchless".to_string(),
                "query".to_string(),
                "-b".to_string(),
                revset.to_string()
            ]]
        );
    }
}

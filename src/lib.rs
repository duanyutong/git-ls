use anstyle::{Ansi256Color, Style};
use clap::{Parser, ValueEnum};
use gix::bstr::ByteSlice as _;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::num::ParseIntError;
use std::process::Command;
use thiserror::Error;

const OKABE_PALETTE: [u8; 7] = [214, 45, 35, 220, 32, 202, 176];
const TABLEAU_PALETTE: [u8; 10] = [67, 215, 167, 73, 71, 221, 139, 217, 137, 249];
const DARK2_PALETTE: [u8; 8] = [36, 166, 98, 162, 70, 178, 136, 242];
const SET1_PALETTE: [u8; 9] = [196, 33, 34, 127, 208, 226, 130, 211, 246];
const SET2_PALETTE: [u8; 8] = [79, 209, 110, 176, 149, 220, 180, 249];
const PAIRED_PALETTE: [u8; 12] = [153, 32, 150, 34, 210, 196, 215, 208, 183, 97, 228, 130];
const BOLD_PALETTE: [u8; 12] = [91, 36, 67, 220, 168, 107, 208, 30, 163, 209, 60, 145];
const VIVID_PALETTE: [u8; 12] = [208, 61, 73, 149, 170, 30, 178, 32, 97, 203, 162, 145];
const TOL_PALETTE: [u8; 7] = [67, 203, 29, 179, 81, 125, 250];
const CLASSIC_PALETTE: [u8; 7] = [41, 203, 45, 220, 176, 33, 214];
const DEFAULT_PALETTE: Palette = Palette::Classic;
const MAIN_SPINE_GLYPH: &str = "│";
const COLLAPSED_MAIN_GLYPH: &str = "⁝";
const MAIN_COMMIT_GLYPH: &str = "◇";
const CURRENT_MAIN_COMMIT_GLYPH: &str = "◆";
const ORPHANED_BRANCH_GLYPH: &str = "⦸";
const TREE_LEFT_PADDING: &str = "";
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
struct LaneGroup {
    base_oid: Option<String>,
    base_meta: Option<CommitMeta>,
    main_distance: Option<usize>,
    lanes: Vec<Lane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RepositorySnapshot {
    current_branch: Option<String>,
    head: Option<String>,
    main_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BuiltLanes {
    Empty {
        main_name: String,
        current_branch: Option<String>,
    },
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
enum Palette {
    #[value(name = "okabe")]
    Okabe,
    Tableau,
    Dark2,
    Set1,
    Set2,
    Paired,
    Bold,
    Vivid,
    Tol,
    Classic,
}

impl Palette {
    fn name(self) -> &'static str {
        match self {
            Self::Okabe => "okabe",
            Self::Tableau => "tableau",
            Self::Dark2 => "dark2",
            Self::Set1 => "set1",
            Self::Set2 => "set2",
            Self::Paired => "paired",
            Self::Bold => "bold",
            Self::Vivid => "vivid",
            Self::Tol => "tol",
            Self::Classic => "classic",
        }
    }

    fn ansi_colours(self) -> &'static [u8] {
        match self {
            Self::Okabe => &OKABE_PALETTE,
            Self::Tableau => &TABLEAU_PALETTE,
            Self::Dark2 => &DARK2_PALETTE,
            Self::Set1 => &SET1_PALETTE,
            Self::Set2 => &SET2_PALETTE,
            Self::Paired => &PAIRED_PALETTE,
            Self::Bold => &BOLD_PALETTE,
            Self::Vivid => &VIVID_PALETTE,
            Self::Tol => &TOL_PALETTE,
            Self::Classic => &CLASSIC_PALETTE,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        DEFAULT_PALETTE
    }
}

impl fmt::Display for Palette {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
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

    #[arg(
        short = 'p',
        long,
        value_enum,
        default_value_t = DEFAULT_PALETTE,
        value_name = "VALUE"
    )]
    palette: Palette,
}

#[derive(Debug)]
struct Colours {
    enabled: bool,
    palette: &'static [u8],
}

impl Colours {
    fn new(mode: ColourMode, palette: Palette) -> Self {
        let enabled = match mode {
            ColourMode::Auto => std::io::stdout().is_terminal(),
            ColourMode::Always => true,
            ColourMode::Never => false,
        };
        Self {
            enabled,
            palette: palette.ansi_colours(),
        }
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
            Ansi256Color(self.palette[index % self.palette.len()]).on_default(),
        )
    }

    fn current_stack(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold()
                .underline(),
        )
    }

    fn current_indicator(&self, index: usize, text: &str) -> String {
        self.paint(
            text,
            Ansi256Color(self.palette[index % self.palette.len()])
                .on_default()
                .bold(),
        )
    }

    fn dim(&self, text: &str) -> String {
        self.paint(text, Style::new().dimmed())
    }

    fn orphaned(&self, text: &str) -> String {
        self.paint(text, Ansi256Color(250).on_default())
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
        let (_head, current_branch) = git.current_head_and_branch()?;
        return Ok(BuiltLanes::Empty {
            main_name: git.main_branch_name()?,
            current_branch,
        });
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

fn build_lane_groups<G: GitBackend + ?Sized>(
    git: &G,
    lanes: Vec<Lane>,
    main_oid: &str,
    meta_cache: &mut HashMap<String, CommitMeta>,
) -> Result<Vec<LaneGroup>> {
    let mut groups = Vec::new();
    for (base_oid, lanes) in grouped_by_base(lanes) {
        let base_meta = match base_oid.as_deref() {
            Some(base_oid) if base_oid != main_oid => {
                Some(get_commit_meta(git, base_oid, meta_cache)?)
            }
            _ => None,
        };
        let main_distance = match base_oid.as_deref() {
            Some(base_oid) if base_oid == main_oid => Some(0),
            Some(base_oid) => {
                let path = git.ancestry_path(Some(base_oid), main_oid)?;
                if path.is_empty() {
                    None
                } else {
                    Some(path.len())
                }
            }
            None => None,
        };

        groups.push(LaneGroup {
            base_oid,
            base_meta,
            main_distance,
            lanes,
        });
    }

    groups.sort_by(lane_group_order);
    Ok(groups)
}

fn lane_group_order(lhs: &LaneGroup, rhs: &LaneGroup) -> Ordering {
    match (lhs.main_distance, rhs.main_distance) {
        (Some(lhs_distance), Some(rhs_distance)) => lhs_distance
            .cmp(&rhs_distance)
            .then_with(|| lane_group_fallback_order(lhs, rhs)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => lane_group_fallback_order(lhs, rhs),
    }
}

fn lane_group_fallback_order(lhs: &LaneGroup, rhs: &LaneGroup) -> Ordering {
    let lhs_contains_current = lhs.lanes.iter().any(|lane| lane.contains_current);
    let rhs_contains_current = rhs.lanes.iter().any(|lane| lane.contains_current);
    (!lhs_contains_current)
        .cmp(&!rhs_contains_current)
        .then_with(|| rhs.lanes.len().cmp(&lhs.lanes.len()))
        .then_with(|| {
            lhs.base_oid
                .as_deref()
                .unwrap_or("")
                .cmp(rhs.base_oid.as_deref().unwrap_or(""))
        })
}

fn marker_for(
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
) -> &'static str {
    if is_current_branch_point(point, current_branch) {
        "●"
    } else if head.is_some_and(|head| point.oid == head) {
        "◉"
    } else {
        "◯"
    }
}

fn is_current_branch_point(point: &BranchPoint, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| point.names.iter().any(|name| name == branch))
}

fn display_names(
    point: &BranchPoint,
    current_branch: Option<&str>,
    colour_index: usize,
    colours: &Colours,
) -> String {
    point
        .names
        .iter()
        .map(|name| {
            if current_branch.is_some_and(|branch| branch == name) {
                format!("  {}", colours.current_stack(colour_index, name))
            } else {
                format!("  {}", colours.stack(colour_index, name))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_orphaned_names(point: &BranchPoint, colours: &Colours) -> String {
    point
        .names
        .iter()
        .map(|name| format!("  {}", colours.orphaned(name)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn current_row_indicator(is_current: bool, colour_index: usize, colours: &Colours) -> String {
    if is_current {
        colours.current_indicator(colour_index, "▶")
    } else {
        " ".to_string()
    }
}

fn orphaned_row_indicator(is_current: bool, colours: &Colours) -> String {
    if is_current {
        colours.orphaned("▶")
    } else {
        " ".to_string()
    }
}

fn render_row(indicator: &str, content: &str) -> String {
    format!("{indicator} {content}")
}

fn row_prefix(
    lane_index: usize,
    lane_count: usize,
    colour_offset: usize,
    point: &BranchPoint,
    current_branch: Option<&str>,
    head: Option<&str>,
    main_spine: MainSpine,
    colours: &Colours,
) -> String {
    let mut slots = Vec::new();
    match main_spine {
        MainSpine::Hidden => {}
        MainSpine::Future => {
            slots.push(" ".to_string());
        }
        MainSpine::FutureLine => {
            slots.push(colours.dim(COLLAPSED_MAIN_GLYPH));
        }
        MainSpine::Connected => {
            slots.push(colours.dim(MAIN_SPINE_GLYPH));
        }
    }
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
    format!("{TREE_LEFT_PADDING}{}", slots.join(" "))
}

fn main_is_current(main_name: &str, current_branch: Option<&str>) -> bool {
    current_branch.is_some_and(|branch| branch == main_name)
}

fn main_label(main_name: &str, current_branch: Option<&str>, colours: &Colours) -> String {
    if main_is_current(main_name, current_branch) {
        format!("  {}", colours.current_stack(0, main_name))
    } else {
        format!("  {}", colours.dim(main_name))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainSpine {
    Hidden,
    Future,
    FutureLine,
    Connected,
}

impl MainSpine {
    fn is_connected(self) -> bool {
        matches!(self, Self::Future | Self::FutureLine | Self::Connected)
    }
}

fn trunk_prefix(
    lane_count: usize,
    colour_offset: usize,
    main_is_current: bool,
    main_spine: MainSpine,
    colours: &Colours,
) -> String {
    let marker = if main_is_current {
        colours.stack(0, CURRENT_MAIN_COMMIT_GLYPH)
    } else {
        colours.dim(MAIN_COMMIT_GLYPH)
    };

    if lane_count == 0 {
        return format!("{TREE_LEFT_PADDING}{marker}{}", colours.dim("──"));
    }

    if !main_spine.is_connected() {
        return format!("{TREE_LEFT_PADDING}{marker}");
    }

    let mut parts = vec![marker];
    for index in 0..lane_count {
        let glyph = if index + 1 == lane_count {
            "─┘"
        } else {
            "─┴"
        };
        parts.push(colours.stack(colour_offset + index, glyph));
    }
    format!("{TREE_LEFT_PADDING}{}", parts.join(""))
}

#[derive(Clone, Copy)]
enum TrunkLabel<'a> {
    Main,
    Commit(&'a CommitMeta),
}

fn trunk_label(label: TrunkLabel<'_>, ctx: &RenderContext<'_>) -> String {
    match label {
        TrunkLabel::Main => main_label(ctx.main_name, ctx.current_branch, ctx.colours),
        TrunkLabel::Commit(meta) => format!("  {}", ctx.colours.dim(&meta.subject)),
    }
}

fn render_main_tip(ctx: &RenderContext<'_>) -> String {
    let current_main = main_is_current(ctx.main_name, ctx.current_branch);
    let line = format!(
        "{}  {}",
        trunk_prefix(0, 0, current_main, MainSpine::Hidden, ctx.colours),
        main_label(ctx.main_name, ctx.current_branch, ctx.colours)
    );
    render_row(&current_row_indicator(current_main, 0, ctx.colours), &line)
}

fn render_top_spacer(colours: &Colours, has_visible_rows_above_main: bool) -> String {
    if has_visible_rows_above_main {
        String::new()
    } else {
        render_omitted_main_past(colours)
    }
}

struct RenderContext<'a> {
    main_name: &'a str,
    current_branch: Option<&'a str>,
    head: Option<&'a str>,
    colours: &'a Colours,
}

fn render_group(
    lanes: &[Lane],
    colour_offset: usize,
    ctx: &RenderContext<'_>,
    label: TrunkLabel<'_>,
    main_spine: MainSpine,
) -> Vec<String> {
    let lane_count = lanes.len();
    let point_count: usize = lanes.iter().map(|lane| lane.branch_points.len()).sum();
    let mut rendered_points = 0;
    let mut output = Vec::new();

    for (lane_index, lane) in lanes.iter().enumerate() {
        for point in &lane.branch_points {
            rendered_points += 1;
            let row_main_spine =
                if matches!(main_spine, MainSpine::Future) && rendered_points == point_count {
                    MainSpine::FutureLine
                } else {
                    main_spine
                };
            let colour_index = colour_offset + lane_index;
            let prefix = row_prefix(
                lane_index,
                lane_count,
                colour_offset,
                point,
                ctx.current_branch,
                ctx.head,
                row_main_spine,
                ctx.colours,
            );
            let label = display_names(point, ctx.current_branch, colour_index, ctx.colours);
            let line = format!("{prefix}  {label}");
            output.push(render_row(
                &current_row_indicator(
                    is_current_branch_point(point, ctx.current_branch),
                    colour_index,
                    ctx.colours,
                ),
                &line,
            ));
        }
    }

    let current_main =
        matches!(label, TrunkLabel::Main) && main_is_current(ctx.main_name, ctx.current_branch);
    let label = trunk_label(label, ctx);
    let line = format!(
        "{}  {}",
        trunk_prefix(
            lane_count,
            colour_offset,
            current_main,
            main_spine,
            ctx.colours
        ),
        label
    );
    output.push(render_row(
        &current_row_indicator(current_main, 0, ctx.colours),
        &line,
    ));
    output
}

fn render_orphaned_group(lanes: &[Lane], ctx: &RenderContext<'_>) -> Vec<String> {
    let mut output = Vec::new();

    for lane in lanes {
        for point in &lane.branch_points {
            let label = display_orphaned_names(point, ctx.colours);
            let status = ctx.colours.orphaned("(orphaned)");
            let line = format!(
                "{TREE_LEFT_PADDING}{} {}  {label}",
                ctx.colours.dim(COLLAPSED_MAIN_GLYPH),
                ctx.colours.orphaned(ORPHANED_BRANCH_GLYPH)
            );
            output.push(render_row(
                &orphaned_row_indicator(
                    is_current_branch_point(point, ctx.current_branch),
                    ctx.colours,
                ),
                &format!("{line} {status}"),
            ));
        }
    }

    output
}

fn render_collapsed_main_segment(
    commit_count: usize,
    ctx: &RenderContext<'_>,
) -> impl IntoIterator<Item = String> {
    let noun = if commit_count == 1 {
        "commit"
    } else {
        "commits"
    };
    let label = format!("({commit_count} {noun} on {})", ctx.main_name);
    [
        render_row(
            " ",
            &format!("{TREE_LEFT_PADDING}{}", ctx.colours.dim(MAIN_SPINE_GLYPH)),
        ),
        render_row(
            " ",
            &format!(
                "{TREE_LEFT_PADDING}{} {}",
                ctx.colours.dim(COLLAPSED_MAIN_GLYPH),
                ctx.colours.dim(&label)
            ),
        ),
        render_row(
            " ",
            &format!("{TREE_LEFT_PADDING}{}", ctx.colours.dim(MAIN_SPINE_GLYPH)),
        ),
    ]
}

fn render_omitted_main_past(colours: &Colours) -> String {
    let line = format!("{TREE_LEFT_PADDING}{}", colours.dim(COLLAPSED_MAIN_GLYPH));
    render_row(" ", &line)
}

fn render_lane_groups(groups: &[LaneGroup], ctx: &RenderContext<'_>) -> Vec<String> {
    let mut output = Vec::new();
    let mut colour_offset = usize::from(main_is_current(ctx.main_name, ctx.current_branch));
    let mut connected_started = false;
    let mut rendered_connected_group = false;
    let mut previous_main_distance = 0;

    for group in groups {
        if let Some(main_distance) = group.main_distance {
            if !connected_started {
                output.push(render_top_spacer(
                    ctx.colours,
                    main_distance == 0 && !group.lanes.is_empty(),
                ));
                if main_distance > 0 {
                    output.push(render_main_tip(ctx));
                }
                connected_started = true;
            }
            if connected_started && main_distance > previous_main_distance {
                output.extend(render_collapsed_main_segment(
                    main_distance - previous_main_distance,
                    ctx,
                ));
            }

            let label = match (main_distance, group.base_meta.as_ref()) {
                (0, _) | (_, None) => TrunkLabel::Main,
                (_, Some(base_meta)) => TrunkLabel::Commit(base_meta),
            };
            let main_spine = if main_distance == 0 {
                MainSpine::Future
            } else {
                MainSpine::Connected
            };
            output.extend(render_group(
                &group.lanes,
                colour_offset,
                ctx,
                label,
                main_spine,
            ));
            rendered_connected_group = true;
            previous_main_distance = main_distance;
            connected_started = true;
        } else {
            if !rendered_connected_group && !connected_started {
                output.push(render_top_spacer(ctx.colours, false));
                output.push(render_main_tip(ctx));
                connected_started = true;
            }
            output.extend(render_orphaned_group(&group.lanes, ctx));
            continue;
        }

        colour_offset += group.lanes.len();
    }

    if !output.is_empty() {
        output.push(render_omitted_main_past(ctx.colours));
    }

    output
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
    let colours = Colours::new(args.colour_mode, args.palette);

    let mut meta_cache = HashMap::new();
    let (lanes, main_oid, repository) = match build_lanes(git, args, &mut meta_cache)? {
        BuiltLanes::Empty {
            main_name,
            current_branch,
        } => {
            let current_main = main_is_current(&main_name, current_branch.as_deref());
            writeln!(stdout, "{}", render_top_spacer(&colours, false))?;
            let main_line = format!(
                "{}  {}",
                trunk_prefix(0, 0, current_main, MainSpine::Hidden, &colours),
                main_label(&main_name, current_branch.as_deref(), &colours)
            );
            writeln!(
                stdout,
                "{}",
                render_row(
                    &current_row_indicator(current_main, 0, &colours),
                    &main_line
                )
            )?;
            writeln!(stdout, "{}", render_omitted_main_past(&colours))?;
            return Ok(());
        }
        BuiltLanes::Populated {
            lanes,
            main_oid,
            repository,
        } => (lanes, main_oid, repository),
    };

    let lanes = ordered_lanes(lanes, args.order);
    let groups = build_lane_groups(git, lanes, &main_oid, &mut meta_cache)?;

    let ctx = RenderContext {
        main_name: &repository.main_name,
        current_branch: repository.current_branch.as_deref(),
        head: repository.head.as_deref(),
        colours: &colours,
    };

    for line in render_lane_groups(&groups, &ctx) {
        writeln!(stdout, "{line}")?;
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

    fn meta(oid: &str, subject: &str) -> CommitMeta {
        CommitMeta {
            oid: oid.to_string(),
            short_oid: oid.to_string(),
            subject: subject.to_string(),
            timestamp: 0,
        }
    }

    fn test_colours(enabled: bool) -> Colours {
        Colours {
            enabled,
            palette: DEFAULT_PALETTE.ansi_colours(),
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
                palette: DEFAULT_PALETTE,
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
                "--palette",
                "tableau",
                "draft() & branches(feature/)",
            ])
            .unwrap(),
            Args {
                revset: "draft() & branches(feature/)".to_string(),
                hidden: true,
                backend: Backend::Shell,
                order: Order::Oldest,
                colour_mode: ColourMode::Never,
                palette: Palette::Tableau,
            }
        );
    }

    #[test]
    fn parses_palette_names() {
        assert_eq!(
            parse_args_from(["-p", "classic"]).unwrap().palette,
            Palette::Classic
        );
        assert_eq!(
            parse_args_from(["--palette", "okabe"]).unwrap().palette,
            Palette::Okabe
        );
    }

    #[test]
    fn parses_additional_palette_names() {
        assert_eq!(
            parse_args_from(["-p", "set1"]).unwrap().palette,
            Palette::Set1
        );
        assert_eq!(
            parse_args_from(["-p", "paired"]).unwrap().palette,
            Palette::Paired
        );
        assert_eq!(
            parse_args_from(["-p", "bold"]).unwrap().palette,
            Palette::Bold
        );
        assert_eq!(
            parse_args_from(["-p", "vivid"]).unwrap().palette,
            Palette::Vivid
        );
        assert_eq!(
            parse_args_from(["-p", "tol"]).unwrap().palette,
            Palette::Tol
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
                palette: DEFAULT_PALETTE,
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
            clap_error_kind(parse_args_from(["--palette=maybe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--palette=safe"]).unwrap_err()),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            clap_error_kind(parse_args_from(["--palette=okabe-ito"]).unwrap_err()),
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
    fn builds_lane_groups_with_main_history_distances() {
        let git = MockGit::default()
            .with(
                &[
                    "show",
                    "-s",
                    "--format=%H%x00%h%x00%ct%x00%s%x1e",
                    "--no-walk=unsorted",
                    "old-main",
                ],
                "old-main\x00old\x001700000001\x00old base\x1e",
            )
            .with(
                &[
                    "rev-list",
                    "--reverse",
                    "--ancestry-path",
                    "old-main..main-oid",
                ],
                "main-1\nmain-2\nmain-oid",
            );
        let lanes = vec![
            lane("current", Some("main-oid"), 2, false),
            lane("old", Some("old-main"), 1, true),
        ];
        let mut cache = HashMap::new();

        let groups = build_lane_groups(&git, lanes, "main-oid", &mut cache).unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].base_oid, Some("main-oid".to_string()));
        assert_eq!(groups[0].main_distance, Some(0));
        assert_eq!(groups[0].base_meta, None);
        assert_eq!(groups[1].base_oid, Some("old-main".to_string()));
        assert_eq!(groups[1].main_distance, Some(3));
        assert_eq!(
            groups[1]
                .base_meta
                .as_ref()
                .map(|meta| meta.subject.as_str()),
            Some("old base")
        );
    }

    #[test]
    fn renders_markers_names_and_trunk() {
        let colours = test_colours(false);
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
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("feature/two"),
            head: Some("b"),
            colours: &colours,
        };

        let output = render_group(&lanes, 0, &ctx, TrunkLabel::Main, MainSpine::Future);

        assert_eq!(
            output,
            vec![
                "    ◯      feature/one".to_string(),
                "▶ ⁝ │ ●    feature/two".to_string(),
                "  ◇─┴─┘    main".to_string()
            ]
        );
    }

    #[test]
    fn renders_exactly_one_future_line_above_main_node() {
        let colours = test_colours(false);
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
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("feature/two"),
            head: Some("b"),
            colours: &colours,
        };

        let output = render_group(&lanes, 0, &ctx, TrunkLabel::Main, MainSpine::Future);

        assert_eq!(
            output
                .iter()
                .filter(|line| line.contains(COLLAPSED_MAIN_GLYPH))
                .count(),
            1
        );
        assert_eq!(output[output.len() - 2], "▶ ⁝ │ ●    feature/two");
        assert_eq!(output[output.len() - 1], "  ◇─┴─┘    main");
    }

    #[test]
    fn renders_single_main_based_lane_with_main_spine() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: true,
        }];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("feature/one"),
            head: Some("a"),
            colours: &colours,
        };

        let output = render_group(&lanes, 0, &ctx, TrunkLabel::Main, MainSpine::Future);

        assert_eq!(
            output,
            vec![
                "▶ ⁝ ●    feature/one".to_string(),
                "  ◇─┘    main".to_string()
            ]
        );
    }

    #[test]
    fn renders_current_main_on_trunk_row() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "a".to_string(),
            base_oid: Some("main".to_string()),
            branch_points: vec![point("a", &["feature/one"])],
            head_timestamp: 1,
            contains_current: false,
        }];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("main"),
            head: Some("main"),
            colours: &colours,
        };

        let output = render_group(&lanes, 0, &ctx, TrunkLabel::Main, MainSpine::Future);

        assert_eq!(
            output,
            vec![
                "  ⁝ ◯    feature/one".to_string(),
                "▶ ◆─┘    main".to_string()
            ]
        );
    }

    #[test]
    fn renders_orphaned_lane_with_single_warning_marker() {
        let colours = test_colours(false);
        let lanes = vec![Lane {
            head_oid: "backup".to_string(),
            base_oid: None,
            branch_points: vec![point("backup", &["test-branch-name"])],
            head_timestamp: 1,
            contains_current: false,
        }];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("main"),
            head: Some("main"),
            colours: &colours,
        };

        let output = render_orphaned_group(&lanes, &ctx);

        assert_eq!(
            output,
            vec!["  ⁝ ⦸    test-branch-name (orphaned)".to_string()]
        );
    }

    #[test]
    fn renders_orphaned_only_groups_around_main_tip() {
        let colours = test_colours(false);
        let groups = vec![LaneGroup {
            base_oid: None,
            base_meta: None,
            main_distance: None,
            lanes: vec![Lane {
                head_oid: "backup".to_string(),
                base_oid: None,
                branch_points: vec![point("backup", &["test-branch-name"])],
                head_timestamp: 1,
                contains_current: false,
            }],
        }];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("main"),
            head: Some("main"),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                "  ⁝".to_string(),
                "▶ ◆──    main".to_string(),
                "  ⁝ ⦸    test-branch-name (orphaned)".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn renders_orphaned_groups_below_connected_stacks() {
        let colours = test_colours(false);
        let groups = vec![
            LaneGroup {
                base_oid: Some("main".to_string()),
                base_meta: None,
                main_distance: Some(0),
                lanes: vec![Lane {
                    head_oid: "feature".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature", &["feature/current"])],
                    head_timestamp: 2,
                    contains_current: true,
                }],
            },
            LaneGroup {
                base_oid: None,
                base_meta: None,
                main_distance: None,
                lanes: vec![
                    Lane {
                        head_oid: "orphan-a".to_string(),
                        base_oid: None,
                        branch_points: vec![point("orphan-a", &["orphan-A"])],
                        head_timestamp: 1,
                        contains_current: false,
                    },
                    Lane {
                        head_oid: "orphan-b".to_string(),
                        base_oid: None,
                        branch_points: vec![point("orphan-b", &["orphan-B"])],
                        head_timestamp: 1,
                        contains_current: false,
                    },
                ],
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("feature/current"),
            head: Some("feature"),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                String::new(),
                "▶ ⁝ ●    feature/current".to_string(),
                "  ◇─┘    main".to_string(),
                "  ⁝ ⦸    orphan-A (orphaned)".to_string(),
                "  ⁝ ⦸    orphan-B (orphaned)".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn renders_old_main_groups_with_collapsed_main_history() {
        let colours = test_colours(false);
        let groups = vec![
            LaneGroup {
                base_oid: Some("main".to_string()),
                base_meta: Some(meta("main", "main tip")),
                main_distance: Some(0),
                lanes: vec![Lane {
                    head_oid: "feature".to_string(),
                    base_oid: Some("main".to_string()),
                    branch_points: vec![point("feature", &["feature/current"])],
                    head_timestamp: 2,
                    contains_current: false,
                }],
            },
            LaneGroup {
                base_oid: Some("old-main".to_string()),
                base_meta: Some(meta(
                    "old-main",
                    "chore: this is an old commit in main history",
                )),
                main_distance: Some(842),
                lanes: vec![Lane {
                    head_oid: "old-feature".to_string(),
                    base_oid: Some("old-main".to_string()),
                    branch_points: vec![point("old-feature", &["dyt/tgs_api"])],
                    head_timestamp: 1,
                    contains_current: true,
                }],
            },
        ];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("dyt/tgs_api"),
            head: Some("old-feature"),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert_eq!(
            output,
            vec![
                String::new(),
                "  ⁝ ◯    feature/current".to_string(),
                "  ◇─┘    main".to_string(),
                "  │".to_string(),
                "  ⁝ (842 commits on main)".to_string(),
                "  │".to_string(),
                "▶ │ ●    dyt/tgs_api".to_string(),
                "  ◇─┘    chore: this is an old commit in main history".to_string(),
                "  ⁝".to_string(),
            ]
        );
    }

    #[test]
    fn colours_text_when_enabled() {
        let colours = test_colours(true);

        assert_eq!(colours.stack(0, "x"), "\x1b[38;5;41mx\x1b[0m");
        assert_eq!(
            colours.current_stack(0, "x"),
            "\x1b[1m\x1b[4m\x1b[38;5;41mx\x1b[0m"
        );
        assert_eq!(
            colours.current_indicator(0, "x"),
            "\x1b[1m\x1b[38;5;41mx\x1b[0m"
        );
        assert_eq!(colours.dim("x"), "\x1b[2mx\x1b[0m");
        assert_eq!(colours.orphaned("x"), "\x1b[38;5;250mx\x1b[0m");
        assert_eq!(
            main_label("main", Some("main"), &colours),
            "  \x1b[1m\x1b[4m\x1b[38;5;41mmain\x1b[0m"
        );
        assert_eq!(
            trunk_prefix(0, 0, true, MainSpine::Hidden, &colours),
            "\x1b[38;5;41m◆\x1b[0m\x1b[2m──\x1b[0m"
        );
    }

    #[test]
    fn active_main_reserves_first_palette_colour() {
        let colours = test_colours(true);
        let groups = vec![LaneGroup {
            base_oid: Some("main".to_string()),
            base_meta: None,
            main_distance: Some(0),
            lanes: vec![Lane {
                head_oid: "feature".to_string(),
                base_oid: Some("main".to_string()),
                branch_points: vec![point("feature", &["feature/one"])],
                head_timestamp: 1,
                contains_current: false,
            }],
        }];
        let ctx = RenderContext {
            main_name: "main",
            current_branch: Some("main"),
            head: Some("main"),
            colours: &colours,
        };

        let output = render_lane_groups(&groups, &ctx);

        assert!(output[1].contains("\x1b[38;5;203m◯\x1b[0m"));
        assert!(output[2].contains("\x1b[38;5;41m◆\x1b[0m"));
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
            palette: DEFAULT_PALETTE,
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
    fn run_renders_empty_selection_as_trunk() {
        let revset = "((draft()) & branches()) - public()";
        let git = MockGit::default()
            .with(&["branchless", "query", "-b", revset], "")
            .with(
                &["rev-parse", "HEAD", "--abbrev-ref", "HEAD"],
                "main-oid\nmain",
            )
            .with(&["config", "--get", "branchless.core.mainBranch"], "");
        let mut output = Vec::new();

        run(["--color", "never"], &git, &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "  ⁝\n▶ ◆──    main\n  ⁝\n"
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
            vec![
                vec![
                    "branchless".to_string(),
                    "query".to_string(),
                    "-b".to_string(),
                    revset.to_string()
                ],
                vec![
                    "rev-parse".to_string(),
                    "HEAD".to_string(),
                    "--abbrev-ref".to_string(),
                    "HEAD".to_string()
                ],
                vec![
                    "config".to_string(),
                    "--get".to_string(),
                    "branchless.core.mainBranch".to_string()
                ]
            ]
        );
    }
}

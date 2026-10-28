use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use gix::bstr::ByteSlice as _;
use serde::{Deserialize, Serialize};

use crate::backend::{GitBackend, Inputs, Session, get_commit_meta};
use crate::cache::{BackendSemantics, Facts, FactsKey, Store};
use crate::cli::{Args, Backend, DEFAULT_REVSET, RuntimeOptions};
use crate::error::Result;
use crate::lanes::{mainline_repository, ordered_lanes, plain_git_main_branch, sort_lane_groups};
use crate::model::{BranchAnnotation, LaneGroup, RepositorySnapshot};
use crate::terminal::RenderEnvironment;

use super::{
    Analysis, build_analysis, execute, hydrate_diff_stats, render_analysis, write_render_plan,
};

#[derive(Serialize, Deserialize)]
struct Snapshot {
    #[serde(with = "crate::cache::topology")]
    groups: Vec<LaneGroup>,
}

impl Snapshot {
    fn neutral(mut groups: Vec<LaneGroup>) -> Self {
        for group in &mut groups {
            group.base_meta = None;
            for lane in &mut group.lanes {
                for point in &mut lane.branch_points {
                    point.annotation = None;
                }
            }
        }
        Self { groups }
    }

    fn live(
        mut self,
        args: &RuntimeOptions,
        repository: RepositorySnapshot,
        main: &str,
        git: &dyn GitBackend,
    ) -> Result<Analysis> {
        let mut metadata = HashMap::new();
        let mut ids = Vec::new();
        if args.verbosity.includes_metadata() {
            ids.push(main);
        }
        for group in &self.groups {
            ids.extend(group.base_oid.as_deref().filter(|base| *base != main));
            for lane in &group.lanes {
                ids.extend(
                    lane.branch_points
                        .iter()
                        .filter(|point| args.verbosity.includes_metadata() || point.main_history)
                        .map(|point| point.oid.as_str()),
                );
            }
        }
        if !ids.is_empty() {
            let _perf = crate::diagnostics::stage("cache_metadata");
            ids.sort_unstable();
            ids.dedup();
            git.cache_commit_metas(&ids, &mut metadata)?;
        }
        for group in &mut self.groups {
            if let Some(base) = group.base_oid.as_deref().filter(|base| *base != main) {
                group.base_meta = Some(get_commit_meta(git, base, &mut metadata)?);
            }
            for lane in &mut group.lanes {
                for point in &mut lane.branch_points {
                    if args.verbosity.includes_metadata() || point.main_history {
                        point.annotation = Some(BranchAnnotation::new(
                            get_commit_meta(git, &point.oid, &mut metadata)?,
                            point.commit_count,
                        ));
                    }
                }
            }
            group.lanes = ordered_lanes(std::mem::take(&mut group.lanes), args.order);
            for lane in &mut group.lanes {
                lane.contains_current = lane.branch_points.iter().any(|point| {
                    repository
                        .current_branch
                        .as_ref()
                        .is_some_and(|branch| point.names.contains(branch))
                        || repository.head.as_ref() == Some(&point.oid)
                });
            }
        }
        sort_lane_groups(&mut self.groups, args.order);
        Ok(Analysis {
            groups: self.groups,
            main_meta: if args.verbosity.includes_metadata() {
                Some(get_commit_meta(git, main, &mut metadata)?)
            } else {
                None
            },
            repository,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CacheLocation {
    common: PathBuf,
    object_format: String,
    configured_main: String,
    shallow_boundary: Vec<String>,
}

impl CacheLocation {
    fn matches_backend(&self, git: &dyn GitBackend) -> bool {
        git.captured_shallow_boundary()
            .is_none_or(|boundary| boundary == self.shallow_boundary)
    }
}

/// Graph facts are valid only for the captured shallow boundary.
/// Other unsupported graph interpretations continue to bypass persistent reuse.
fn location(directory: impl AsRef<std::path::Path>) -> Option<CacheLocation> {
    for name in [
        "GIT_NAMESPACE",
        "GIT_REPLACE_REF_BASE",
        "GIT_SHALLOW_FILE",
        "GIT_GRAFT_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        if std::env::var_os(name).is_some() {
            return None;
        }
    }
    let repo = gix::discover_with_environment_overrides(directory).ok()?;
    let common = repo.common_dir().canonicalize().ok()?;
    if common.join("info/grafts").exists() || common.join("objects/info/alternates").exists() {
        return None;
    }
    let config = repo.config_snapshot();
    if config.string("extensions.partialClone").is_some()
        || config.string("gitoxide.core.shallowFile").is_some()
    {
        return None;
    }
    // Promisor declarations may occur in included configuration files.
    if config
        .plumbing()
        .sections_by_name("remote")
        .is_some_and(|mut sections| sections.any(|section| section.contains_value_name("promisor")))
    {
        return None;
    }
    if repo
        .references()
        .ok()?
        .prefixed("refs/replace/")
        .ok()?
        .next()
        .is_some()
    {
        return None;
    }
    let configured_main = config.string("branchless.core.mainBranch").map_or_else(
        || "main".to_string(),
        |value| value.to_str_lossy().into_owned(),
    );
    let mut shallow_boundary: Vec<_> = repo.shallow_commits().ok()?.map_or_else(Vec::new, |ids| {
        ids.iter().map(ToString::to_string).collect()
    });
    shallow_boundary.sort_unstable();
    shallow_boundary.dedup();
    Some(CacheLocation {
        common,
        object_format: repo.object_hash().to_string(),
        configured_main,
        shallow_boundary,
    })
}

fn prepare_cache(
    raw: &Args,
    args: &RuntimeOptions,
    git: &dyn GitBackend,
) -> Option<(Store, CacheLocation)> {
    if raw.clear_cache {
        let cleared = gix::discover_with_environment_overrides(".")
            .ok()
            .and_then(|repo| Store::open(repo.common_dir()))
            .is_some_and(|store| store.clear());
        crate::diagnostics::event(format_args!(
            "cache_clear={}",
            if cleared {
                "complete"
            } else {
                "deferred_busy_or_unavailable"
            }
        ));
        return None;
    }
    if raw.no_cache {
        crate::diagnostics::event(format_args!("cache=disabled"));
        return None;
    }
    if args.revset.trim() != DEFAULT_REVSET || args.hidden {
        crate::diagnostics::event(format_args!("cache=bypass reason=branchless_state"));
        return None;
    }
    let context = {
        let _perf = crate::diagnostics::stage("cache_validation");
        location(".")
    };
    let Some(context) = context else {
        crate::diagnostics::event(format_args!(
            "cache=bypass reason=unsupported_graph_or_store"
        ));
        return None;
    };
    if !context.matches_backend(git) {
        crate::diagnostics::event(format_args!("cache=bypass reason=shallow_boundary_changed"));
        return None;
    }
    let Some(store) = Store::open(&context.common) else {
        crate::diagnostics::event(format_args!("cache=bypass reason=unavailable"));
        return None;
    };
    crate::diagnostics::event(format_args!(
        "cache_schema=1 cache_path={} entry_limit_bytes={} snapshot_slots=32",
        store.root.display(),
        crate::cache::ENTRY_LIMIT
    ));
    crate::diagnostics::event(format_args!(
        "cache_graph={} shallow_boundaries={}",
        if context.shallow_boundary.is_empty() {
            "complete"
        } else {
            "shallow"
        },
        context.shallow_boundary.len()
    ));
    Some((store, context))
}

fn admit_facts(facts: &mut Facts, key: &str) -> Option<Vec<u8>> {
    let _perf = crate::diagnostics::stage("cache_admission");
    let encoded = std::cell::RefCell::new(None);
    let before = facts.metadata.len();
    facts.fit(4096, &|value| {
        let bytes = crate::cache::encoded_entry(key, value);
        let fits = bytes.is_some();
        encoded.replace(bytes);
        fits
    });
    crate::diagnostics::event(format_args!(
        "cache_admission entry=facts metadata_before={} metadata_retained={}",
        before,
        facts.metadata.len()
    ));
    crate::diagnostics::count("retained_diff_contexts", facts.diff.scopes.len());
    crate::diagnostics::count(
        "retained_diff_pairs",
        facts
            .diff
            .scopes
            .values()
            .map(|scope| scope.values.len())
            .sum(),
    );
    crate::diagnostics::count(
        "retained_diff_paths",
        facts
            .diff
            .scopes
            .values()
            .flat_map(|scope| scope.values.values())
            .map(|value| value.paths.len())
            .sum(),
    );
    encoded.into_inner()
}

pub(super) fn execute_cached<W: Write>(
    raw: &Args,
    args: &RuntimeOptions,
    git: &dyn GitBackend,
    stdout: &mut W,
    environment: RenderEnvironment,
) -> Result<()> {
    let Some((store, context)) = prepare_cache(raw, args, git) else {
        return execute(args, git, stdout, environment);
    };
    let inputs = Inputs::capture(git)?;
    let (main_name, local_oid) = plain_git_main_branch(
        &inputs.branches.clone().into_iter().collect(),
        &inputs.configured_main,
    )?;
    let backend = match args.backend {
        Backend::Gix => BackendSemantics::Gix,
        Backend::Shell => BackendSemantics::Shell,
    };
    let compatibility = FactsKey::new(
        backend,
        &context.common,
        &context.object_format,
        &context.shallow_boundary,
    );
    let Some((facts_key, key)) = compatibility.encode().zip(compatibility.snapshot(&inputs)) else {
        crate::diagnostics::event(format_args!("cache=bypass reason=unrepresentable_key"));
        return execute(args, git, stdout, environment);
    };
    let name = Store::snapshot_name(&key);
    let facts_name = match args.backend {
        Backend::Gix => "facts-gix.json",
        Backend::Shell => "facts-shell.json",
    };
    let snapshot: Option<Snapshot> = if raw.refresh_cache {
        None
    } else {
        store.read(&name, &key)
    };
    let topology_hit = snapshot.is_some();
    let facts: Facts = if raw.refresh_cache {
        Facts::default()
    } else {
        store.read(facts_name, &facts_key).unwrap_or_default()
    };
    let original_facts = facts.clone();
    let session = Session::with_facts(git, inputs.clone(), facts);
    let (main_oid, repository) = mainline_repository(&session, &main_name, &local_oid)?;
    let (analysis, computed, prepared) = if let Some(snapshot) = snapshot {
        crate::diagnostics::event(format_args!("cache=hit"));
        let mut analysis = snapshot.live(args, repository, &main_oid, &session)?;
        let prepared = hydrate_diff_stats(args, &mut analysis, &session, environment)?;
        let facts = session.has_new_facts().then(|| session.into_facts());
        (analysis, facts, prepared)
    } else {
        crate::diagnostics::event(format_args!(
            "cache={} reason={}",
            if store.root.join(&name).exists() && !raw.refresh_cache {
                "invalid"
            } else {
                "miss"
            },
            if raw.refresh_cache {
                "refresh"
            } else {
                "absent_or_key_mismatch"
            }
        ));
        let mut analysis = build_analysis(args, &session)?;
        let prepared = hydrate_diff_stats(args, &mut analysis, &session, environment)?;
        (analysis, Some(session.into_facts()), prepared)
    };
    let plan = render_analysis(args, &analysis, environment, &prepared);
    write_render_plan(stdout, &plan, environment)?;
    let mut published = false;
    if let Some(mut facts) = computed {
        let encoded = admit_facts(&mut facts, &facts_key);
        if topology_hit && facts.same_answers(&original_facts) {
            crate::diagnostics::event(format_args!(
                "cache_publication=skipped reason=unchanged_answers"
            ));
            crate::cache::maintenance::after_render(&store, false, raw.no_cache_maintenance, &name);
            return Ok(());
        }
        let unchanged = {
            let _perf = crate::diagnostics::stage("cache_revalidation");
            Inputs::capture(git).is_ok_and(|current| current == inputs)
                && context.configured_main == inputs.configured_main
                && location(".").as_ref() == Some(&context)
        };
        if unchanged {
            if let Some(bytes) = encoded {
                store.publish_encoded(facts_name, &bytes);
            }
            if !topology_hit {
                published = store.publish(&name, &key, &Snapshot::neutral(analysis.groups));
            }
        } else {
            crate::diagnostics::event(format_args!(
                "cache_publication=discarded reason=inputs_changed"
            ));
        }
    }
    crate::cache::maintenance::after_render(&store, published, raw.no_cache_maintenance, &name);
    Ok(())
}

#[cfg(test)]
#[path = "tests/cached.rs"]
mod repositories;

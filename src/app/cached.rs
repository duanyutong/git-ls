use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::backend::{Facts, GitBackend, Inputs, Session};
use crate::cache::Store;
use crate::cli::{Args, Backend, DEFAULT_REVSET, RuntimeOptions, Verbosity};
use crate::error::Result;
use crate::lanes::{ordered_lanes, plain_git_main_branch, sort_lane_groups};
use crate::model::{CommitMeta, LaneGroup, RepositorySnapshot};
use crate::terminal::RenderEnvironment;

use super::{Analysis, build_analysis, execute, render_analysis, write_render_plan};

#[derive(Serialize, Deserialize)]
struct Snapshot {
    groups: Vec<LaneGroup>,
    main_meta: Option<CommitMeta>,
}

/// Eligibility is intentionally conservative. Unusual graph interpretation must
/// never reuse answers established against the ordinary object graph.
fn location() -> Option<(PathBuf, String)> {
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
    let repo = gix::discover_with_environment_overrides(".").ok()?;
    let common = repo.common_dir().canonicalize().ok()?;
    if repo.is_shallow()
        || common.join("info/grafts").exists()
        || common.join("objects/info/alternates").exists()
    {
        return None;
    }
    let config = repo.config_snapshot();
    if config.string("extensions.partialClone").is_some()
        || config.string("gitoxide.core.shallowFile").is_some()
    {
        return None;
    }
    // Promisor declarations may occur in included configuration files.
    let config_text = config.plumbing().to_string().to_ascii_lowercase();
    if config_text.contains("promisor") {
        return None;
    }
    for reference in repo.references().ok()?.all().ok()? {
        let reference = reference.ok()?;
        if reference.name().as_bstr().starts_with(b"refs/replace/") {
            return None;
        }
    }
    Some((common, format!("{:?}", repo.object_hash())))
}

fn prepare_cache(raw: &Args, args: &RuntimeOptions) -> Option<(Store, PathBuf, String)> {
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
        location()
    };
    let Some((common, object_format)) = context else {
        crate::diagnostics::event(format_args!(
            "cache=bypass reason=unsupported_graph_or_store"
        ));
        return None;
    };
    let Some(store) = Store::open(&common) else {
        crate::diagnostics::event(format_args!("cache=bypass reason=unavailable"));
        return None;
    };
    crate::diagnostics::event(format_args!(
        "cache_schema=1 cache_path={} entry_limit_bytes={} snapshot_slots=32",
        store.root.display(),
        crate::cache::ENTRY_LIMIT
    ));
    Some((store, common, object_format))
}

pub(super) fn execute_cached<W: Write>(
    raw: &Args,
    args: &RuntimeOptions,
    git: &dyn GitBackend,
    stdout: &mut W,
    environment: RenderEnvironment,
) -> Result<()> {
    let Some((store, common, object_format)) = prepare_cache(raw, args) else {
        return execute(args, git, stdout, environment);
    };
    let inputs = Inputs::capture(git)?;
    let (main_name, _) = plain_git_main_branch(
        &inputs.branches.clone().into_iter().collect(),
        &inputs.configured_main,
    )?;
    let backend = match args.backend {
        Backend::Gix => "gix",
        Backend::Shell => "shell",
    };
    let facts_key = format!(
        "analysis-1:{backend}:gix-0.88.0:{object_format}:{}",
        common.display()
    );
    let key = format!("{facts_key}:{inputs:?}");
    let name = Store::snapshot_name(&key);
    let facts_name = format!("facts-{backend}.json");
    let snapshot: Option<Snapshot> = if raw.refresh_cache {
        None
    } else {
        store.read(&name, &key)
    };
    let (head, current_branch) = git.current_head_and_branch()?;
    let repository = RepositorySnapshot::new(current_branch, head, main_name);
    let (analysis, computed) = if let Some(mut snapshot) = snapshot {
        crate::diagnostics::event(format_args!("cache=hit"));
        for group in &mut snapshot.groups {
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
        sort_lane_groups(&mut snapshot.groups, args.order);
        (
            Analysis {
                groups: snapshot.groups,
                main_meta: snapshot.main_meta,
                repository,
            },
            None,
        )
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
        let facts: Facts = if raw.refresh_cache {
            Facts::default()
        } else {
            store.read(&facts_name, &facts_key).unwrap_or_default()
        };
        let session = Session::with_facts(git, inputs.clone(), facts);
        let mut complete = args.clone();
        complete.verbosity = Verbosity::High;
        let analysis = build_analysis(&complete, &session)?;
        (analysis, Some(session.facts()))
    };
    let plan = render_analysis(args, &analysis, environment);
    write_render_plan(stdout, &plan, environment)?;
    stdout.flush()?;
    if let Some(facts) = computed {
        let unchanged = {
            let _perf = crate::diagnostics::stage("cache_revalidation");
            Inputs::capture(git).is_ok_and(|current| current == inputs)
                && location().as_ref() == Some(&(common, object_format))
        };
        if unchanged {
            store.publish(&facts_name, &facts_key, &facts);
            store.publish(
                &name,
                &key,
                &Snapshot {
                    groups: analysis.groups,
                    main_meta: analysis.main_meta,
                },
            );
        } else {
            crate::diagnostics::event(format_args!(
                "cache_publication=discarded reason=inputs_changed"
            ));
        }
    }
    Ok(())
}

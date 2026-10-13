use std::io::Write;
use std::path::PathBuf;

use gix::bstr::ByteSlice as _;
use serde::{Deserialize, Serialize};

use crate::backend::{GitBackend, Inputs, Session};
use crate::cache::{BackendSemantics, Facts, FactsKey, Store};
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

impl Snapshot {
    fn live(mut self, args: &RuntimeOptions, repository: RepositorySnapshot) -> Analysis {
        for group in &mut self.groups {
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
        Analysis {
            groups: self.groups,
            main_meta: self.main_meta,
            repository,
        }
    }
}

/// Eligibility is intentionally conservative. Unusual graph interpretation must
/// never reuse answers established against the ordinary object graph.
fn location(directory: impl AsRef<std::path::Path>) -> Option<(PathBuf, String, String)> {
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
    if repo.is_shallow().ok()?
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
    Some((common, repo.object_hash().to_string(), configured_main))
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
        location(".")
    };
    let Some((common, object_format, _)) = context else {
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
        Backend::Gix => BackendSemantics::Gix,
        Backend::Shell => BackendSemantics::Shell,
    };
    let compatibility = FactsKey::new(backend, &common, &object_format);
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
    let (head, current_branch) = git.current_head_and_branch()?;
    let repository = RepositorySnapshot::new(current_branch, head, main_name);
    let (analysis, computed) = if let Some(snapshot) = snapshot {
        crate::diagnostics::event(format_args!("cache=hit"));
        (snapshot.live(args, repository), None)
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
            store.read(facts_name, &facts_key).unwrap_or_default()
        };
        let session = Session::with_facts(git, inputs.clone(), facts);
        let mut complete = args.clone();
        complete.verbosity = Verbosity::High;
        let analysis = build_analysis(&complete, &session)?;
        (analysis, Some(session.into_facts()))
    };
    let plan = render_analysis(args, &analysis, environment);
    write_render_plan(stdout, &plan, environment)?;
    let mut published = false;
    if let Some(mut facts) = computed {
        let unchanged = {
            let _perf = crate::diagnostics::stage("cache_revalidation");
            Inputs::capture(git).is_ok_and(|current| current == inputs)
                && location(".").as_ref() == Some(&(common, object_format, inputs.configured_main))
        };
        if unchanged {
            {
                let _perf = crate::diagnostics::stage("cache_admission");
                facts.compact(512);
            }
            store.publish(facts_name, &facts_key, &facts);
            published = store.publish(
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
    crate::cache::maintenance::after_render(&store, published, raw.no_cache_maintenance, &name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_support::TestRepo;

    #[test]
    fn revalidation_observes_configuration_changes_after_backend_creation() {
        let repo = TestRepo::new();
        repo.commit("root");
        repo.git(&["config", "branchless.core.mainBranch", "main"]);
        let backend = gix::discover(repo.path()).unwrap();
        let before = location(repo.path()).unwrap();
        repo.git(&["config", "branchless.core.mainBranch", "trunk"]);
        assert_eq!(
            backend
                .config_snapshot()
                .string("branchless.core.mainBranch")
                .unwrap()
                .to_str_lossy(),
            "main"
        );
        let after = location(repo.path()).unwrap();
        assert_eq!(after.0, before.0);
        assert_eq!(after.1, before.1);
        assert_eq!(after.2, "trunk");
        assert_ne!(before, after);
    }
}

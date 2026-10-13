//! Best-effort maintenance of the application namespace; never Git object GC.
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::policy::{
    MaintenanceState as State, PASS_TIME, should_evict, within_reclamation_budget,
};
use super::{SLOTS, Store, regular};

pub(crate) const PRIVATE_MODE: &str = "--internal-cache-maintenance";
const STATE: &str = "maintenance-state.json";
const REPORT: &str = "maintenance-report.json";
#[derive(Default, Serialize, Deserialize)]
struct Report {
    completed_at: u64,
    duration_ms: f64,
    entries_examined: usize,
    entries_removed: usize,
    bytes_reclaimed: u64,
    #[serde(default)]
    pending: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn after_render(store: &Store, published: bool, disabled: bool, retain: &str) {
    let _perf = crate::diagnostics::stage("cache_maintenance_schedule");
    if disabled {
        crate::diagnostics::event(format_args!("maintenance=disabled"));
        return;
    }
    if crate::diagnostics::enabled()
        && let Some(report) = store.read::<Report>(REPORT, REPORT)
    {
        crate::diagnostics::event(format_args!(
            "maintenance_previous completed_at={} duration_ms={:.3} entries_examined={} entries_removed={} bytes_reclaimed={} pending={}",
            report.completed_at,
            report.duration_ms,
            report.entries_examined,
            report.entries_removed,
            report.bytes_reclaimed,
            report.pending
        ));
    }
    let Some(lock) = store.lock("publication.lock") else {
        crate::diagnostics::event(format_args!("maintenance=deferred reason=busy"));
        return;
    };
    let mut state: State = store.read(STATE, STATE).unwrap_or_default();
    let bytes = if published {
        fs::metadata(store.root.join(retain)).map_or(0, |metadata| metadata.len())
    } else {
        0
    };
    state.record(published, bytes);
    let current = now();
    if !state.due(current) {
        if published {
            store.publish_locked(&lock, STATE, STATE, &state);
        }
        crate::diagnostics::event(format_args!(
            "maintenance=not_due publications={} admitted_bytes={}",
            state.publications, state.bytes_published
        ));
        return;
    }
    // Reserving the attempt under the publication lock coalesces worktrees.
    // An abandoned reservation becomes eligible after the minimum interval.
    state.reserve(current);
    if !store.publish_locked(&lock, STATE, STATE, &state) {
        crate::diagnostics::event(format_args!(
            "maintenance=deferred reason=reservation_failed"
        ));
        return;
    }
    drop(lock);
    let scheduled = spawn(store, retain);
    crate::diagnostics::event(format_args!(
        "maintenance={}",
        if scheduled {
            "scheduled"
        } else {
            "deferred_spawn_failed"
        }
    ));
}

fn spawn(store: &Store, retain: &str) -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("nice");
        command.args(["-n", "10"]).arg(executable);
        command
    };
    #[cfg(not(unix))]
    let mut command = Command::new(executable);
    command.arg(PRIVATE_MODE).arg(&store.root).arg(retain);
    detached(&mut command);
    command.spawn().is_ok()
}

fn detached(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | BELOW_NORMAL_PRIORITY_CLASS
        command.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0000_4000);
    }
}

pub(crate) fn run(root: &Path, retain: &str) {
    // Accept only the namespace produced by Store::open; symlink components and
    // unrelated paths must never become maintenance targets.
    if !root.ends_with("git-ls/cache/v1") {
        return;
    }
    let Some(common) = root.ancestors().nth(3) else {
        return;
    };
    let Some(store) = Store::open(common) else {
        return;
    };
    if store.root != root {
        return;
    }
    let Some(_worker) = store.lock("maintenance.lock") else {
        return;
    };
    let Some(lock) = store.lock("publication.lock") else {
        return;
    };
    let started = Instant::now();
    let mut report = Report::default();
    let current = now();
    let mut entries = Vec::new();
    for slot in 0..SLOTS {
        if started.elapsed() >= PASS_TIME {
            report.pending = true;
            break;
        }
        let name = format!("snapshot-{slot:02}.json");
        let path = store.root.join(&name);
        if !regular(&path) {
            continue;
        }
        if let Ok(metadata) = fs::metadata(&path) {
            report.entries_examined += 1;
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            entries.push((modified, name, metadata.len()));
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (index, (modified, name, bytes)) in entries.into_iter().enumerate() {
        let modified = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if !should_evict(index, modified, bytes, name == retain, current) {
            continue;
        }
        if started.elapsed() >= PASS_TIME
            || !within_reclamation_budget(report.bytes_reclaimed, bytes)
        {
            report.pending = true;
            continue;
        }
        if fs::remove_file(store.root.join(name)).is_ok() {
            report.entries_removed += 1;
            report.bytes_reclaimed += bytes;
        } else {
            report.pending = true;
        }
    }
    // No publisher can be active while this lock is held.
    let _ = fs::remove_file(store.root.join("publication.tmp"));
    report.duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    report.completed_at = now();
    store.publish_locked(&lock, REPORT, REPORT, &report);
    store.publish_locked(
        &lock,
        STATE,
        STATE,
        &State::completed(report.completed_at, report.pending),
    );
}

#[cfg(test)]
#[path = "tests/maintenance.rs"]
mod repositories;

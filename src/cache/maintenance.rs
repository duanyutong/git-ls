//! Best-effort maintenance of the application namespace; never Git object GC.
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{ENTRY_LIMIT, SLOTS, Store, regular};

pub(crate) const PRIVATE_MODE: &str = "--internal-cache-maintenance";
const STATE: &str = "maintenance-state.json";
const REPORT: &str = "maintenance-report.json";
const INTERVAL: u64 = 3600;
const MAX_AGE: Duration = Duration::from_hours(168);
const PASS_TIME: Duration = Duration::from_millis(100);
const PASS_BYTES: u64 = 8 * ENTRY_LIMIT;

#[derive(Default, Serialize, Deserialize)]
struct State {
    publications: u64,
    last_attempt: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Report {
    completed_at: u64,
    duration_ms: f64,
    entries_examined: usize,
    entries_removed: usize,
    bytes_reclaimed: u64,
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
            "maintenance_previous completed_at={} duration_ms={:.3} entries_examined={} entries_removed={} bytes_reclaimed={}",
            report.completed_at,
            report.duration_ms,
            report.entries_examined,
            report.entries_removed,
            report.bytes_reclaimed
        ));
    }
    let Some(lock) = store.lock("publication.lock") else {
        crate::diagnostics::event(format_args!("maintenance=deferred reason=busy"));
        return;
    };
    let mut state: State = store.read(STATE, STATE).unwrap_or_default();
    state.publications = state.publications.saturating_add(u64::from(published));
    let current = now();
    if state.publications < 16 || current.saturating_sub(state.last_attempt) < INTERVAL {
        if published {
            store.publish_locked(&lock, STATE, STATE, &state);
        }
        crate::diagnostics::event(format_args!("maintenance=not_due"));
        return;
    }
    // Reserving the attempt under the publication lock coalesces worktrees.
    // An abandoned reservation becomes eligible after the minimum interval.
    state.last_attempt = current;
    state.publications = 16;
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
    let mut entries = Vec::new();
    for slot in 0..SLOTS {
        if started.elapsed() >= PASS_TIME {
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
        if started.elapsed() >= PASS_TIME
            || report.bytes_reclaimed.saturating_add(bytes) > PASS_BYTES
        {
            break;
        }
        if name == retain {
            continue;
        }
        let expired = modified.elapsed().is_ok_and(|age| age >= MAX_AGE);
        if (expired || index >= 8 || bytes > ENTRY_LIMIT)
            && fs::remove_file(store.root.join(name)).is_ok()
        {
            report.entries_removed += 1;
            report.bytes_reclaimed += bytes;
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
        &State {
            publications: 0,
            last_attempt: report.completed_at,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_obeys_locks_retention_and_byte_budget() {
        let common = tempfile::tempdir().unwrap();
        let store = Store::open(common.path()).unwrap();
        for slot in 0..SLOTS {
            assert!(store.publish(&format!("snapshot-{slot:02}.json"), "key", &slot));
        }
        let lock = store.lock("maintenance.lock").unwrap();
        run(&store.root, "snapshot-00.json");
        assert!(store.read::<Report>(REPORT, REPORT).is_none());
        drop(lock);
        run(&store.root, "snapshot-00.json");
        let report: Report = store.read(REPORT, REPORT).unwrap();
        assert_eq!(report.entries_examined, 32);
        assert!(report.entries_removed >= 23);
        assert!(report.bytes_reclaimed <= PASS_BYTES);
        assert!(store.root.join("snapshot-00.json").exists());
        let state: State = store.read(STATE, STATE).unwrap();
        assert_eq!(state.publications, 0);
        assert!(state.last_attempt > 0);
        run(common.path(), "");
    }

    #[test]
    fn scheduling_is_disabled_or_coalesced_without_waiting() {
        let common = tempfile::tempdir().unwrap();
        let store = Store::open(common.path()).unwrap();
        after_render(&store, true, true, "");
        assert!(store.read::<State>(STATE, STATE).is_none());
        let lock = store.lock("publication.lock").unwrap();
        after_render(&store, true, false, "");
        drop(lock);
        after_render(&store, true, false, "");
        assert_eq!(store.read::<State>(STATE, STATE).unwrap().publications, 1);
        store.publish(
            STATE,
            STATE,
            &State {
                publications: 100,
                last_attempt: now(),
            },
        );
        after_render(&store, false, false, "");
        assert_eq!(store.read::<State>(STATE, STATE).unwrap().publications, 100);
    }
}

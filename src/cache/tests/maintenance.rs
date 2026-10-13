use super::*;
use crate::cache::ENTRY_LIMIT;
use crate::cache::policy::PASS_BYTES;

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
fn large_cache_eviction_is_bounded_and_never_removes_the_active_publication() {
    let common = tempfile::tempdir().unwrap();
    let store = Store::open(common.path()).unwrap();
    for slot in 0..SLOTS {
        let path = store.root.join(format!("snapshot-{slot:02}.json"));
        fs::write(&path, vec![b'x'; usize::try_from(ENTRY_LIMIT).unwrap()]).unwrap();
        fs::File::open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
    }
    let temporary = store.root.join("publication.tmp");
    fs::write(&temporary, "in progress").unwrap();
    let lock = store.lock("publication.lock").unwrap();
    run(&store.root, "snapshot-00.json");
    assert!(temporary.exists());
    assert!(store.read::<Report>(REPORT, REPORT).is_none());
    drop(lock);
    run(&store.root, "snapshot-00.json");
    let report: Report = store.read(REPORT, REPORT).unwrap();
    assert!(report.entries_removed <= 8);
    assert!(report.bytes_reclaimed <= PASS_BYTES);
    assert!(store.root.join("snapshot-00.json").exists());
    assert!(!temporary.exists());
    // Another pass can make progress after an interrupted or budgeted pass.
    run(&store.root, "snapshot-00.json");
    assert!(store.root.join("snapshot-00.json").exists());
    assert!(
        store
            .read::<Report>(REPORT, REPORT)
            .unwrap()
            .bytes_reclaimed
            <= PASS_BYTES
    );
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
            bytes_published: 0,
        },
    );
    after_render(&store, false, false, "");
    assert_eq!(store.read::<State>(STATE, STATE).unwrap().publications, 100);
}

#[test]
fn oversized_retained_or_deferred_entries_do_not_block_expired_entries() {
    for retained in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let large = store.root.join("snapshot-00.json");
        fs::File::create(&large)
            .unwrap()
            .set_len(PASS_BYTES + 1)
            .unwrap();
        let small = store.root.join("snapshot-01.json");
        fs::write(&small, "expired").unwrap();
        fs::File::open(&small)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
        run(&store.root, if retained { "snapshot-00.json" } else { "" });
        assert!(large.exists());
        assert!(!small.exists());
        let report: Report = store.read(REPORT, REPORT).unwrap();
        assert_eq!(report.entries_removed, 1);
        assert_eq!(report.bytes_reclaimed, 7);
        assert_eq!(report.pending, !retained);
        let state: State = store.read(STATE, STATE).unwrap();
        assert_eq!(state.publications, if retained { 0 } else { 16 });
        assert_eq!(state.due(state.last_attempt + 3600), !retained);
    }
}

use super::*;

#[test]
fn publishes_atomically_and_validates_keys_versions_and_checksums() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let name = Store::snapshot_name("key");
    assert!(store.publish(&name, "key", &vec!["one", "two"]));
    assert_eq!(
        store.read::<Vec<String>>(&name, "key").unwrap(),
        vec!["one", "two"]
    );
    assert_eq!(store.read::<Vec<String>>(&name, "wrong"), None);
    assert_eq!(store.read::<Vec<String>>("missing", "key"), None);
    let path = store.root.join(&name);
    let original = fs::read(&path).unwrap();
    for change in ["checksum", "semantics", "payload"] {
        let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
        json[change] = if change == "checksum" {
            serde_json::json!(0)
        } else {
            serde_json::json!("corrupt")
        };
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(store.read::<Vec<String>>(&name, "key"), None);
    }
    fs::write(&path, b"{truncated").unwrap();
    assert_eq!(store.read::<Vec<String>>(&name, "key"), None);
    File::create(&path)
        .unwrap()
        .set_len(ENTRY_LIMIT + 1)
        .unwrap();
    assert_eq!(store.read::<Vec<String>>(&name, "key"), None);
}

#[test]
fn bounded_publication_and_clearing_do_not_wait_for_active_writers() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let lock = store.lock("publication.lock").unwrap();
    assert!(!store.publish("facts-gix.json", "key", &42));
    assert!(!store.clear());
    drop(lock);
    assert!(!store.publish(
        "facts-gix.json",
        "key",
        &"x".repeat(usize::try_from(ENTRY_LIMIT).unwrap() + 1)
    ));
    fs::write(store.root.join("publication.tmp"), "interrupted").unwrap();
    assert!(store.publish("facts-gix.json", "key", &42));
    assert_eq!(store.read::<u64>("facts-gix.json", "key"), Some(42));
    fs::write(store.root.join("foreign"), "retain").unwrap();
    assert!(store.clear());
    assert_eq!(store.read::<u64>("facts-gix.json", "key"), None);
    assert!(store.root.join("foreign").exists());
}

#[test]
fn slot_count_bounds_history_and_unwritable_paths_are_optional() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    for key in 0..100 {
        let key = key.to_string();
        assert!(store.publish(&Store::snapshot_name(&key), &key, &key));
    }
    assert!(fs::read_dir(&store.root).unwrap().count() <= usize::try_from(SLOTS).unwrap() + 1);
    assert!(Store::open(&directory.path().join("missing")).is_none());
    fs::write(directory.path().join("file"), "not a directory").unwrap();
    assert!(Store::open(&directory.path().join("file")).is_none());
}

#[cfg(unix)]
#[test]
fn refuses_symlink_namespaces_and_entries() {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(other.path(), directory.path().join("git-ls")).unwrap();
    assert!(Store::open(directory.path()).is_none());
    let store = Store::open(other.path()).unwrap();
    std::os::unix::fs::symlink(directory.path(), store.root.join("publication.lock")).unwrap();
    assert!(!store.publish("facts-gix.json", "key", &42));
    std::os::unix::fs::symlink(directory.path(), store.root.join("facts-gix.json")).unwrap();
    assert_eq!(store.read::<u64>("facts-gix.json", "key"), None);
}

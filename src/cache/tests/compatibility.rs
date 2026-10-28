use super::*;
use std::collections::BTreeMap;

#[test]
fn canonical_keys_include_compatibility_and_every_selection_dependency() {
    let inputs = Inputs {
        upstreams: std::collections::BTreeMap::new(),
        branches: BTreeMap::from([("oid".into(), vec!["topic".into()])]),
        configured_main: "main".into(),
    };
    let native = FactsKey::new(BackendSemantics::Gix, Path::new("/repo"), "sha1", &[]);
    let key = native.snapshot(&inputs).unwrap();
    let json: serde_json::Value = serde_json::from_str(&key).unwrap();
    assert_eq!(json["compatibility"]["analysis"], ANALYSIS_SEMANTICS);
    assert_eq!(json["compatibility"]["backend"], "gix");
    assert_eq!(
        json["compatibility"]["shallow_boundary"],
        serde_json::json!([])
    );
    assert_eq!(json["inputs"]["configured_main"], "main");
    for alternative in [
        FactsKey::new(BackendSemantics::Shell, Path::new("/repo"), "sha1", &[]),
        FactsKey::new(BackendSemantics::Gix, Path::new("/other"), "sha1", &[]),
        FactsKey::new(BackendSemantics::Gix, Path::new("/repo"), "sha256", &[]),
        FactsKey::new(
            BackendSemantics::Gix,
            Path::new("/repo"),
            "sha1",
            &["boundary".into()],
        ),
    ] {
        assert_ne!(alternative.encode(), native.encode());
        assert_ne!(alternative.snapshot(&inputs), Some(key.clone()));
    }
    let mut changed = inputs.clone();
    changed.configured_main = "trunk".into();
    assert_ne!(native.snapshot(&changed), Some(key.clone()));
    changed = inputs.clone();
    changed.branches.insert("new".into(), vec!["topic".into()]);
    assert_ne!(native.snapshot(&changed), Some(key.clone()));
    changed = inputs;
    changed.branches.get_mut("oid").unwrap()[0] = "renamed".into();
    assert_ne!(native.snapshot(&changed), Some(key));
}

#[cfg(unix)]
#[test]
fn unrepresentable_paths_bypass_keys_without_lossy_aliasing() {
    use std::os::unix::ffi::OsStringExt;
    let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![255]));
    let key = FactsKey::new(BackendSemantics::Gix, &path, "sha1", &[]);
    assert_eq!(key.encode(), None);
    assert_eq!(
        key.snapshot(&Inputs {
            upstreams: std::collections::BTreeMap::new(),
            branches: BTreeMap::new(),
            configured_main: "main".into()
        }),
        None
    );
}

#[test]
fn upstream_identity_and_target_participate_in_snapshot_compatibility() {
    let mut inputs = Inputs {
        upstreams: BTreeMap::new(),
        branches: BTreeMap::new(),
        configured_main: "main".into(),
    };
    let key = FactsKey::new(BackendSemantics::Gix, Path::new("/repo"), "sha1", &[]);
    let absent = key.snapshot(&inputs);
    inputs.upstreams.insert(
        "main".into(),
        crate::model::MainReference {
            name: "origin/main".into(),
            oid: "remote".into(),
            cached: true,
        },
    );
    let original = key.snapshot(&inputs);
    assert_ne!(original, absent);
    inputs.upstreams.get_mut("main").unwrap().oid = "new-remote".into();
    assert_ne!(key.snapshot(&inputs), original);
    inputs.upstreams.get_mut("main").unwrap().name = "upstream/trunk".into();
    assert_ne!(key.snapshot(&inputs), original);
}

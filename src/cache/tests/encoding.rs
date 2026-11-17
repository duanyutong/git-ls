use super::*;

fn fixture() -> DiffFacts {
    let paths = Rc::new(vec!["a".repeat(220), "b".repeat(220)]);
    let mut facts = DiffFacts::default();
    for index in 0..8 {
        facts.scopes.entry(0).or_default().values.insert(
            format!("pair-{index}"),
            DiffValue {
                stat: DiffStat {
                    added: index,
                    deleted: 0,
                },
                paths: paths.clone(),
                fingerprint: 1,
                configuration: 2,
                used: index,
            },
        );
    }
    facts
}

#[test]
fn persistent_dependencies_are_interned_and_decoded_sets_share_allocations() {
    let facts = fixture();
    let encoded = serde_json::to_vec(&facts).unwrap();
    assert!(encoded.len() < 8 * 440);
    let wire: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(wire["paths"].as_array().unwrap().len(), 2);
    assert_eq!(wire["dependencies"].as_array().unwrap().len(), 1);
    let decoded: DiffFacts = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, facts);
    assert!(Rc::ptr_eq(
        &decoded.scopes[&0].values["pair-0"].paths,
        &decoded.scopes[&0].values["pair-1"].paths,
    ));
    let mut distinct = facts;
    distinct
        .scopes
        .get_mut(&0)
        .unwrap()
        .values
        .get_mut("pair-0")
        .unwrap()
        .paths = Rc::new(vec!["b".repeat(220), "c".repeat(220)]);
    let wire = serde_json::to_value(&distinct).unwrap();
    assert_eq!(wire["paths"].as_array().unwrap().len(), 3);
    assert_eq!(wire["dependencies"].as_array().unwrap().len(), 2);
    assert_eq!(serde_json::from_value::<DiffFacts>(wire).unwrap(), distinct);
    distinct.scopes.get_mut(&0).unwrap().values.remove("pair-0");
    let wire = serde_json::to_value(&distinct).unwrap();
    assert_eq!(wire["paths"].as_array().unwrap().len(), 2);
    assert_eq!(wire["dependencies"].as_array().unwrap().len(), 1);
    let empty = DiffFacts::default();
    assert_eq!(
        serde_json::from_value::<DiffFacts>(serde_json::to_value(&empty).unwrap()).unwrap(),
        empty
    );
}

#[test]
fn canonical_pool_sizes_remain_monotonic_when_comparisons_are_added() {
    let mut facts = DiffFacts::default();
    let mut previous = serde_json::to_vec(&facts).unwrap().len();
    for index in (0..32).rev() {
        facts.scopes.entry(0).or_default().values.insert(
            format!("pair-{index:02}"),
            DiffValue {
                stat: DiffStat::default(),
                paths: Rc::new(vec!["common".into(), format!("path-{index:02}")]),
                fingerprint: 1,
                configuration: 1,
                used: index,
            },
        );
        let size = serde_json::to_vec(&facts).unwrap().len();
        assert!(size >= previous, "{size} < {previous}");
        previous = size;
        let restored: DiffFacts =
            serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
        assert_eq!(restored, facts);
    }
}

#[test]
fn invalid_pool_references_and_malformed_structures_are_rejected() {
    let wire = serde_json::to_value(fixture()).unwrap();
    let mut path = wire.clone();
    path["dependencies"][0][0] = serde_json::json!(999);
    assert!(
        serde_json::from_value::<DiffFacts>(path)
            .unwrap_err()
            .to_string()
            .contains("invalid dependency path")
    );
    let mut dependency = wire;
    dependency["scopes"]["0"]["values"]["pair-0"]["paths"] = serde_json::json!(999);
    assert!(
        serde_json::from_value::<DiffFacts>(dependency)
            .unwrap_err()
            .to_string()
            .contains("invalid dependency set")
    );
    assert!(serde_json::from_str::<DiffFacts>("{}").is_err());
    assert!(serde_json::from_slice::<DiffFacts>(b"{}").is_err());
    assert!(serde_json::from_value::<DiffFacts>(serde_json::json!({})).is_err());
    let paths = vec!["unregistered".into()];
    assert!(Pools::default().intern(&paths).is_err());
}

#[test]
fn context_and_comparison_limits_apply_to_encoding_and_decoding() {
    let mut facts = fixture();
    for context in 1..5 {
        facts.scopes.insert(context, DiffScope::default());
    }
    assert!(serde_json::to_vec(&facts).is_err());
    assert!(serde_json::to_value(&facts).is_err());
    let mut wire = serde_json::to_value(fixture()).unwrap();
    for context in 1..5 {
        wire["scopes"][context.to_string()] = serde_json::json!({"values": {}, "used": 0});
    }
    assert!(serde_json::from_value::<DiffFacts>(wire).is_err());
    let mut facts = fixture();
    let sample = facts.scopes[&0].values["pair-0"].clone();
    let mut wire = serde_json::to_value(&facts).unwrap();
    let encoded = wire["scopes"]["0"]["values"]["pair-0"].clone();
    for index in 0..4097 {
        facts
            .scopes
            .get_mut(&0)
            .unwrap()
            .values
            .insert(format!("extra-{index}"), sample.clone());
        wire["scopes"]["0"]["values"][format!("extra-{index}")] = encoded.clone();
    }
    assert!(serde_json::to_vec(&facts).is_err());
    assert!(serde_json::to_value(&facts).is_err());
    assert!(serde_json::from_value::<DiffFacts>(wire).is_err());
}

#[test]
fn pooled_references_cannot_expand_beyond_the_memory_budget() {
    let mut facts = fixture();
    facts
        .scopes
        .get_mut(&0)
        .unwrap()
        .values
        .get_mut("pair-0")
        .unwrap()
        .paths = Rc::new(vec!["x".repeat(200 * 1024); 82]);
    assert!(
        serde_json::to_vec(&facts)
            .unwrap_err()
            .to_string()
            .contains("memory bound")
    );
    assert!(serde_json::to_value(&facts).is_err());
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["paths"] = serde_json::json!(["x".repeat(200 * 1024)]);
    wire["dependencies"] = serde_json::json!([vec![0; 82]]);
    assert!(
        serde_json::from_value::<DiffFacts>(wire)
            .unwrap_err()
            .to_string()
            .contains("memory bound")
    );
}

#[test]
fn content_interning_is_independent_of_runtime_allocation_sharing() {
    let original = fixture();
    let mut separate = original.clone();
    for value in separate.scopes.get_mut(&0).unwrap().values.values_mut() {
        value.paths = Rc::new(value.paths.as_ref().clone());
    }
    assert_eq!(
        serde_json::to_value(&separate).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
}

#[test]
fn decoding_accounts_for_reserved_slots_before_allocating_the_next_dependency_set() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["paths"] = serde_json::json!(["x".repeat(500_000), ""]);
    wire["dependencies"] = serde_json::json!([vec![0; 33], vec![1; 100_000]]);
    assert!(crate::cache::encoded_entry("key", &wire).is_some());
    assert!(
        serde_json::from_value::<DiffFacts>(wire)
            .unwrap_err()
            .to_string()
            .contains("memory bound")
    );
}

#[test]
fn unused_dependency_pools_obey_the_representation_count_bound() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["dependencies"] = serde_json::json!(vec![Vec::<usize>::new(); 4097]);
    assert!(serde_json::from_value::<DiffFacts>(wire).is_err());
}

#[test]
fn dependency_references_follow_canonical_order_after_reverse_insertion() {
    let mut facts = fixture();
    let values = &mut facts.scopes.get_mut(&0).unwrap().values;
    values.get_mut("pair-0").unwrap().paths = Rc::new(vec!["z".into()]);
    values.get_mut("pair-1").unwrap().paths = Rc::new(vec!["a".into()]);
    let mut pools = Pools::new(&facts);
    let z = pools
        .intern(&facts.scopes[&0].values["pair-0"].paths)
        .unwrap();
    let a = pools
        .intern(&facts.scopes[&0].values["pair-1"].paths)
        .unwrap();
    let (paths, dependencies) = pools.finish();
    let a: usize = serde_json::from_value(serde_json::to_value(a).unwrap()).unwrap();
    let z: usize = serde_json::from_value(serde_json::to_value(z).unwrap()).unwrap();
    assert_eq!((a, z), (0, 1));
    assert_eq!(paths[dependencies[a][0]], "a");
    assert_eq!(paths[dependencies[z][0]], "z");
}

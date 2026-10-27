use super::*;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    #[serde(with = "super")]
    groups: Vec<LaneGroup>,
}

#[test]
fn shared_paths_round_trip_without_duplicate_ancestry_or_live_annotations() {
    let common = BranchPoint::new("common", ["trunk"], None);
    let mut lanes = Vec::new();
    for name in ["left", "right"] {
        lanes.push(Lane::new_with_rewritten_commits(
            name,
            Some("base".into()),
            vec![BranchPoint::new(name, [name], None), common.clone()],
            Vec::new(),
            1,
            true,
        ));
    }
    let snapshot = Snapshot {
        groups: vec![LaneGroup::new(Some("base".into()), None, Some(0), lanes)],
    };
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert_eq!(text.matches("\"oid\":\"common\"").count(), 1);
    let restored: Snapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        restored.groups[0].lanes[0].branch_points,
        snapshot.groups[0].lanes[0].branch_points
    );
    assert_eq!(restored.groups[0].lanes[1].head_oid, "right");
    assert!(!restored.groups[0].lanes[1].contains_current);
    let empty = Snapshot { groups: Vec::new() };
    assert_eq!(
        serde_json::from_slice::<Snapshot>(&serde_json::to_vec(&empty).unwrap())
            .unwrap()
            .groups,
        Vec::new()
    );
}

#[test]
fn invalid_or_cyclic_paths_are_rejected() {
    let point = BranchPoint::new("point", ["branch"], None);
    for (tip, parent) in [(Some(10), None), (Some(0), Some(0)), (Some(0), Some(1))] {
        let topology = Topology {
            points: vec![(point.clone(), parent)],
            groups: vec![Group {
                base: None,
                distance: None,
                lanes: vec![Path {
                    head: "head".into(),
                    base: None,
                    tip,
                    rewritten: Vec::new(),
                    timestamp: 0,
                }],
            }],
        };
        let text = format!(
            "{{\"groups\":{}}}",
            serde_json::to_string(&topology).unwrap()
        );
        assert!(serde_json::from_str::<Snapshot>(&text).is_err());
    }
    assert!(serde_json::from_str::<Snapshot>("{\"groups\":{}}").is_err());
}

#[test]
fn empty_lanes_and_groups_preserve_their_snapshot_boundaries() {
    let snapshot = Snapshot {
        groups: vec![
            LaneGroup::new(
                None,
                None,
                None,
                vec![Lane::new_with_rewritten_commits(
                    "head",
                    None,
                    Vec::new(),
                    Vec::new(),
                    0,
                    false,
                )],
            ),
            LaneGroup::new(None, None, None, Vec::new()),
        ],
    };
    let restored: Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(restored.groups, snapshot.groups);
}

#[test]
fn string_and_byte_decoders_both_reject_corruption_and_accept_valid_paths() {
    let snapshot = Snapshot {
        groups: vec![LaneGroup::new(
            None,
            None,
            None,
            vec![Lane::new_with_rewritten_commits(
                "head",
                None,
                vec![BranchPoint::new("head", ["topic"], None)],
                Vec::new(),
                0,
                false,
            )],
        )],
    };
    let text = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<Snapshot>(&text).unwrap().groups,
        snapshot.groups
    );
    assert!(serde_json::from_slice::<Snapshot>(b"{\"groups\":{}}").is_err());
    for tip in [0, 10] {
        let topology = Topology {
            points: vec![(BranchPoint::new("head", ["topic"], None), Some(0))],
            groups: vec![Group {
                base: None,
                distance: None,
                lanes: vec![Path {
                    head: "head".into(),
                    base: None,
                    tip: Some(tip),
                    rewritten: Vec::new(),
                    timestamp: 0,
                }],
            }],
        };
        let text = format!(
            "{{\"groups\":{}}}",
            serde_json::to_string(&topology).unwrap()
        );
        assert!(serde_json::from_slice::<Snapshot>(text.as_bytes()).is_err());
    }
}

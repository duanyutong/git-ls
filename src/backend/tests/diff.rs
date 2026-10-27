use super::*;
use crate::test_support::MockGit;

fn parse_numstat(output: &str) -> Result<DiffStat> {
    Ok(parse_batch(&["header\n".to_string()], &format!("header\n{output}"))?.stats[0])
}

#[test]
fn counts_text_changes_and_skips_binary_files_with_arbitrary_paths() {
    assert_eq!(parse_numstat("").unwrap(), DiffStat::default());
    assert_eq!(
        parse_numstat("12\t3\tpath\twith\nspaces\0-\t-\tbinary\0").unwrap(),
        DiffStat {
            added: 12,
            deleted: 3
        }
    );
    assert_eq!(
        parse_numstat("0\t0\t\0old\0new\0").unwrap(),
        DiffStat::default()
    );
    assert_eq!(
        parse_numstat("1\t2\ta\x001\t2\tb\0").unwrap(),
        DiffStat {
            added: 2,
            deleted: 4
        }
    );
    let batch = parse_batch(
        &["a b\n".into(), "b c\n".into(), "c d\n".into()],
        "a b\nb c\n1\t0\tx\0c d\n",
    )
    .unwrap();
    assert_eq!(
        batch.stats,
        vec![
            DiffStat::default(),
            DiffStat {
                added: 1,
                deleted: 0
            },
            DiffStat::default()
        ]
    );
    assert_eq!(batch.paths, vec!["x"]);
    assert!(parse_batch(&["expected\n".into()], "wrong\n").is_err());
    assert!(parse_batch(&[], "extra").is_err());
}

#[test]
fn rejects_truncated_malformed_and_overflowing_counts() {
    for output in [
        "bad",
        "1\0",
        "1\t\0",
        "0\t0\t\0",
        "0\t0\t\0old\0",
        "1\ta\tpath\0",
        "a\t1\tpath\0",
        "-\t1\tpath\0",
        "18446744073709551616\t0\tp\0",
        "18446744073709551615\t0\tp\x001\t0\tq\0",
        "0\t18446744073709551615\tp\x000\t1\tq\0",
    ] {
        assert!(parse_numstat(output).is_err(), "{output:?}");
    }
}

pub(crate) fn pair(base: Option<&str>, head: &str) -> DiffPair {
    DiffPair {
        base: base.map(str::to_string),
        head: head.to_string(),
    }
}

#[test]
fn batches_endpoint_trees_deduplicates_identical_trees_and_skips_empty_diffs() {
    let a = "1".repeat(40);
    let b = "2".repeat(40);
    let git = MockGit::default()
        .with(
            &["cat-file", "--batch-check=%(objectname)"],
            &format!("{a}\n{b}\n{b}\n"),
        )
        .with(DIFF_ARGS, &format!("{a} {b}\n2\t1\tfile\0"));
    let pairs = vec![
        pair(Some("base"), "head"),
        pair(Some("base"), "other"),
        pair(Some("head"), "other"),
    ];
    let batch = git.diff_stats(&pairs).unwrap();
    assert_eq!(
        batch.stats,
        vec![
            DiffStat {
                added: 2,
                deleted: 1
            },
            DiffStat {
                added: 2,
                deleted: 1
            },
            DiffStat::default()
        ]
    );
    assert_eq!(git.calls().len(), 2);
    assert_eq!(git.diff_stats(&[]).unwrap(), DiffBatch::default());
    assert_eq!(
        super::super::Session::new(&git).diff_stats(&pairs).unwrap(),
        batch
    );
    assert_eq!(GitCommand::diff_context(&git, &[]), None);
    assert_eq!(DiffStatBackend::diff_context(&git, &[]), None);
    assert_eq!(
        DiffStatBackend::diff_context(&super::super::Session::new(&git), &[]),
        None
    );
    let trees = HashMap::from([("a".into(), a)]);
    assert_eq!(
        diff_with_trees(&MockGit::default(), &[pair(Some("a"), "a")], &trees)
            .unwrap()
            .stats,
        vec![DiffStat::default()]
    );
    assert!(diff_with_trees(&git, &[pair(Some("missing"), "a")], &trees).is_err());
    assert!(diff_with_trees(&git, &[pair(Some("a"), "missing")], &trees).is_err());
}

#[test]
fn resolves_the_empty_tree_once_for_multiple_orphans_and_propagates_failures() {
    let a = "1".repeat(40);
    let empty = "0".repeat(40);
    let pairs = [pair(None, "one"), pair(None, "two")];
    let git = MockGit::default()
        .with(
            &["cat-file", "--batch-check=%(objectname)"],
            &format!("{a}\n{a}\n"),
        )
        .with(&["hash-object", "-t", "tree", "--stdin"], &empty)
        .with(DIFF_ARGS, &format!("{empty} {a}\n4\t0\tfile\0"));
    assert_eq!(
        git.diff_stats(&pairs).unwrap().stats,
        vec![
            DiffStat {
                added: 4,
                deleted: 0
            };
            2
        ]
    );
    assert_eq!(git.calls().len(), 3);
    assert!(MockGit::default().diff_stats(&pairs).is_err());
    let git = MockGit::default().with(
        &["cat-file", "--batch-check=%(objectname)"],
        &format!("{a}\n{a}\n"),
    );
    assert!(git.diff_stats(&pairs).is_err());
    let git = MockGit::default().with(
        &["cat-file", "--batch-check=%(objectname)"],
        &format!("{a}\n{empty}\n"),
    );
    assert!(git.diff_stats(&[pair(Some("one"), "two")]).is_err());
}

#[test]
fn validates_resolved_tree_counts_object_formats_and_invalid_objects() {
    for output in [
        "",
        "not-a-tree",
        &"x".repeat(40),
        &format!("{}\n{}\n", "a".repeat(40), "b".repeat(40)),
    ] {
        assert!(parse_trees(["one"].into_iter(), output).is_err());
    }
    let sha256 = "a".repeat(64);
    assert_eq!(
        parse_trees(["one"].into_iter(), &sha256).unwrap()["one"],
        sha256
    );
    let pairs = [pair(Some("one"), "two")];
    assert!(
        MockGit::default()
            .with(&["cat-file", "--batch-check=%(objectname)"], "missing")
            .diff_stats(&pairs)
            .is_err()
    );
    assert!(
        MockGit::default()
            .with(
                &["cat-file", "--batch-check=%(objectname)"],
                &format!("{}\n{}\n", "a".repeat(40), "b".repeat(40))
            )
            .with(DIFF_ARGS, "malformed")
            .diff_stats(&pairs)
            .is_err()
    );
}

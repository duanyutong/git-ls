use std::collections::{HashMap, HashSet};

use crate::error::{GitLsError, Result};

use super::graph::descendant_counts;
use super::shell::lines;
use super::traits::GitCommand;

pub(super) fn unmerged(
    git: &(impl GitCommand + ?Sized),
    main: &str,
    tips: &[String],
) -> Result<Vec<String>> {
    let _perf = crate::diagnostics::operation("select_tips");
    if tips.iter().all(|tip| tip == main) {
        return Ok(Vec::new());
    }
    // Query immutable IDs: filtering live refs could associate an answer with
    // the wrong captured snapshot during a concurrent branch movement.
    let output = git.run_input(&["rev-list", "--stdin"], &revision_input(main, tips))?;
    let selected: HashSet<_> = tips.iter().map(String::as_str).collect();
    let mut result = Vec::new();
    let mut visited = 0;
    for id in output.lines().filter(|id| !id.is_empty()) {
        visited += 1;
        if selected.contains(id) {
            result.push(id.to_string());
        }
    }
    crate::diagnostics::count("selection_vertices", visited);
    result.sort();
    result.dedup();
    Ok(result)
}

fn revision_input(main: &str, tips: &[String]) -> String {
    let mut input = tips.join("\n");
    input.push_str("\n^");
    input.push_str(main);
    input.push('\n');
    input
}

pub(super) fn independent(
    git: &(impl GitCommand + ?Sized),
    main: &str,
    tips: &[String],
) -> Result<Vec<String>> {
    let _perf = crate::diagnostics::operation("independent_heads");
    if tips.is_empty() {
        return Ok(Vec::new());
    }
    let mut heads = if tips.len() <= 128 {
        let mut args = vec!["merge-base", "--independent"];
        args.extend(tips.iter().map(String::as_str));
        lines(&git.run(&args, false)?)
    } else {
        // A shared range walk avoids both argument limits and pairwise chunk reduction.
        let output = git.run_input(
            &["rev-list", "--parents", "--stdin"],
            &revision_input(main, tips),
        )?;
        let candidates: HashSet<_> = tips.iter().map(String::as_str).collect();
        let mut intermediate = HashSet::new();
        for line in output.lines() {
            intermediate.extend(
                line.split_whitespace()
                    .skip(1)
                    .filter(|id| candidates.contains(id)),
            );
        }
        tips.iter()
            .filter(|id| !intermediate.contains(id.as_str()))
            .cloned()
            .collect()
    };
    heads.sort();
    heads.dedup();
    Ok(heads)
}

fn full_oid(id: &str) -> Result<&str> {
    if matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(id)
    } else {
        Err(GitLsError::invalid_object_id(
            id,
            "expected a full SHA-1 or SHA-256 object ID",
        ))
    }
}

pub(super) fn distances(
    git: &(impl GitCommand + ?Sized),
    main: &str,
    bases: &[String],
) -> Result<HashMap<String, Option<usize>>> {
    let _perf = crate::diagnostics::operation("main_distances");
    let historical: Vec<_> = bases
        .iter()
        .filter(|base| !base.eq_ignore_ascii_case(main))
        .collect();
    if historical.is_empty() {
        return Ok(bases.iter().map(|base| (base.clone(), Some(0))).collect());
    }
    let mut boundary = Some(historical[0].clone());
    for base in historical.iter().skip(1) {
        let Some(previous) = boundary else { break };
        let output = git.run(&["merge-base", &previous, base], true)?;
        boundary = super::shell::non_empty(&output);
    }
    let mut args = vec!["rev-list", "--parents", main];
    if let Some(boundary) = &boundary {
        args.extend(["--not", boundary]);
    }
    let mut output = git.run(&args, false)?;
    output.make_ascii_lowercase();
    let mut nodes = Vec::new();
    for line in output.lines() {
        let ids: Vec<_> = line
            .split_whitespace()
            .map(full_oid)
            .collect::<Result<_>>()?;
        if let Some((id, parents)) = ids.split_first() {
            nodes.push((*id, parents.to_vec()));
        }
    }
    let normalised: Vec<_> = bases.iter().map(|base| base.to_ascii_lowercase()).collect();
    let ids: Vec<_> = normalised
        .iter()
        .map(|base| full_oid(base))
        .collect::<Result<_>>()?;
    crate::diagnostics::count("main_vertices", nodes.len());
    let counts = descendant_counts(&nodes, &ids);
    Ok(bases
        .iter()
        .zip(counts)
        .map(|(base, count)| {
            (
                base.clone(),
                if base.eq_ignore_ascii_case(main) {
                    Some(0)
                } else {
                    (count > 0).then_some(count)
                },
            )
        })
        .collect())
}

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
    let output = git.run(
        &[
            "for-each-ref",
            "--format=%(objectname)",
            &format!("--no-merged={main}"),
            "refs/heads",
        ],
        false,
    )?;
    let selected: HashSet<_> = tips.iter().map(String::as_str).collect();
    let mut result: Vec<_> = lines(&output)
        .into_iter()
        .filter(|id| selected.contains(id.as_str()))
        .collect();
    result.sort();
    result.dedup();
    Ok(result)
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
        let mut input = tips.join("\n");
        input.push_str("\n^");
        input.push_str(main);
        input.push('\n');
        let output = git.run_input(&["rev-list", "--parents", "--stdin"], &input)?;
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

pub(super) fn distances(
    git: &(impl GitCommand + ?Sized),
    main: &str,
    bases: &[String],
) -> Result<HashMap<String, Option<usize>>> {
    let _perf = crate::diagnostics::operation("main_distances");
    let historical: Vec<_> = bases.iter().filter(|base| base.as_str() != main).collect();
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
    let output = git.run(&args, false)?;
    let mut nodes = Vec::new();
    for line in output.lines() {
        let ids: Vec<_> = line
            .split_whitespace()
            .map(|id| {
                id.parse::<gix::ObjectId>()
                    .map_err(|error| GitLsError::invalid_object_id(id, error))
            })
            .collect::<Result<_>>()?;
        if let Some((id, parents)) = ids.split_first() {
            nodes.push((*id, parents.to_vec()));
        }
    }
    let ids: Vec<_> = bases
        .iter()
        .map(|base| {
            base.parse::<gix::ObjectId>()
                .map_err(|error| GitLsError::invalid_object_id(base, error))
        })
        .collect::<Result<_>>()?;
    crate::diagnostics::count("main_vertices", nodes.len());
    let counts = descendant_counts(&nodes, &ids);
    Ok(bases
        .iter()
        .zip(counts)
        .map(|(base, count)| {
            (
                base.clone(),
                if base == main {
                    Some(0)
                } else {
                    (count > 0).then_some(count)
                },
            )
        })
        .collect())
}

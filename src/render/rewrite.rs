use crate::model::RewrittenCommit;

use super::context::RenderContext;
use super::metadata::{columns_count, format_age};

pub(super) fn display_rewritten_commit(
    commit: &RewrittenCommit,
    ctx: &RenderContext<'_>,
) -> String {
    let old_oid = ctx.colours.metadata_oid(&commit.meta.short_oid);
    let replacement_oid = ctx.colours.metadata_oid(&commit.replacement.short_oid);

    if !ctx.verbosity.includes_metadata() {
        return format!("{old_oid} rewritten as {replacement_oid}");
    }

    if ctx.layout.is_columns() {
        let count_column = columns_count("", ctx.metadata_widths, ctx.colours, true);
        if !ctx.verbosity.includes_oid() {
            let status = ctx.colours.metadata_punctuation("rewritten");
            return format!("{count_column} {status}");
        }

        let status = ctx.colours.metadata_punctuation("rewritten as");
        let body = format!("{count_column} {old_oid} {status} {replacement_oid}");
        return if ctx.verbosity.includes_title() {
            format!("{body} {}", ctx.colours.commit_title(&commit.meta.subject))
        } else {
            body
        };
    }

    let age = format_age(ctx.now_timestamp, commit.meta.timestamp);
    let age = ctx.colours.metadata_age(&format!(
        "{age:>age_width$}",
        age_width = ctx.metadata_widths.age
    ));
    let open = ctx.colours.metadata_punctuation("(");
    let status = ctx.colours.metadata_count(if ctx.verbosity.includes_oid() {
        "rewritten as"
    } else {
        "rewritten"
    });
    let close = ctx.colours.metadata_punctuation(")");
    if !ctx.verbosity.includes_oid() {
        return format!("{age} {open}{status}{close}");
    }

    let prefix = format!("{age} {open}{status} {replacement_oid}{close}");

    if ctx.verbosity.includes_title() {
        format!(
            "{prefix} {old_oid} {}",
            ctx.colours.commit_title(&commit.meta.subject)
        )
    } else {
        format!("{prefix} {old_oid}")
    }
}
